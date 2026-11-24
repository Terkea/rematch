import { describe, expect, it } from "vitest";
import {
  type BarConfig,
  filterBarModel,
  parseBarFilters,
} from "../../src/app/(app)/app/_filters/model.js";
import type { FormatOption } from "../../src/server/filters/formats.js";

// The shared filter bar's view model (T3): every control is a plain link or a GET form, so the
// bar works without JavaScript. Hrefs go through `serializeFilters`, so they are canonical.

const NOW = new Date("2026-11-22T12:00:00Z");
const FORMATS: FormatOption[] = [
  { key: "600+0", label: "10+0", games: 30 },
  { key: "900+10", label: "15+10", games: 12 },
  { key: "other", label: "Other", games: 3 },
];
const base: BarConfig = {
  basePath: "/app",
  timeClasses: [
    { tc: "blitz", games: 40 },
    { tc: "rapid", games: 120 },
  ],
  formats: FORMATS,
  color: "on",
  dates: true,
  compare: true,
  now: NOW,
};
const parse = (q: string, cfg: Partial<BarConfig> = {}) =>
  parseBarFilters(new URLSearchParams(q), { ...base, ...cfg });
const model = (q: string, cfg: Partial<BarConfig> = {}) => {
  const c = { ...base, ...cfg };
  return filterBarModel(parseBarFilters(new URLSearchParams(q), c), c);
};

describe("time control row", () => {
  it("lists rapid first and makes it the default, with game counts", () => {
    const m = model("");
    expect(m.timeClass.map((o) => [o.label, o.count, o.current])).toEqual([
      ["Rapid", 120, true],
      ["Blitz", 40, false],
    ]);
    expect(m.timeClass[0]?.href).toBe("/app");
    expect(m.timeClass[1]?.href).toBe("/app?tc=blitz");
  });

  it("offers All last when the page allows it, as tc=all", () => {
    const m = model("tc=all", { allTimeClasses: true });
    expect(m.timeClass.at(-1)).toMatchObject({ label: "All", href: "/app?tc=all", current: true });
    expect(m.timeClass.at(-1)?.count).toBe(160);
    // A filter link under All keeps All.
    expect(m.color?.find((o) => o.label === "White")?.href).toBe("/app?tc=all&color=white");
  });

  it("drops the format when the class changes, and keeps colour, dates and comparison", () => {
    const m = model("fmt=600%2B0&color=black&from=2026-10-01&cmp=month");
    expect(m.timeClass[1]?.href).toBe("/app?tc=blitz&from=2026-10-01&color=black&cmp=month");
  });

  it("keeps an explicit All time on every link", () => {
    const m = model("from=all");
    expect(m.timeClass[1]?.href).toBe("/app?tc=blitz&from=all");
    expect(m.color?.[1]?.href).toBe("/app?from=all&color=white");
  });

  it("keeps the page's own params (the coach scope's account) on every link", () => {
    const m = model("", { keep: { account: "a1" } });
    expect(m.timeClass[1]?.href).toBe("/app?tc=blitz&account=a1");
    expect(m.period?.custom.hidden).toContainEqual(["account", "a1"]);
  });
});

describe("format row", () => {
  it("lists All formats, then the class's top formats", () => {
    const m = model("fmt=900%2B10");
    expect(m.format?.map((o) => [o.label, o.current])).toEqual([
      ["All formats", false],
      ["10+0", false],
      ["15+10", true],
      ["Other", false],
    ]);
    expect(m.format?.[1]?.href).toBe("/app?fmt=600%2B0");
  });

  it("is absent without formats, under All, or when the page does not support it", () => {
    expect(model("", { formats: [] }).format).toBeNull();
    expect(model("tc=all", { allTimeClasses: true }).format).toBeNull();
    expect(model("", { formatSupported: false }).format).toBeNull();
  });
});

describe("colour row", () => {
  it("offers Both, White and Black", () => {
    const m = model("color=white");
    expect(m.color?.map((o) => [o.label, o.current, o.href])).toEqual([
      ["Both", false, "/app"],
      ["White", true, "/app?color=white"],
      ["Black", false, "/app?color=black"],
    ]);
  });

  it("is replaced by an honest coming-soon note where the stored data has no colour split", () => {
    const m = model("color=white", { color: "coming" });
    expect(m.color).toBeNull();
    expect(m.colorComing).toBe(true);
    // The colour still reads from the URL, so the page can say it is not applied.
    expect(parse("color=white", { color: "coming" }).color).toBe("white");
  });
});

describe("period row", () => {
  it("opens on the last 90 days, writes presets as days and All time as from=all", () => {
    const m = model("");
    const labels = m.period?.options.map((o) => [o.label, o.current]);
    expect(labels).toEqual([
      ["30 days", false],
      ["90 days", true],
      ["6 months", false],
      ["12 months", false],
      ["All time", false],
    ]);
    expect(m.period?.options[0]?.href).toBe("/app?from=2026-10-24&to=2026-11-22");
    expect(m.period?.options[1]?.href).toBe("/app");
    expect(m.period?.options[4]?.href).toBe("/app?from=all");
    expect(model("from=all").period?.options[4]?.current).toBe(true);
    // The 90 days written out are the default, so they name the same preset.
    expect(model("from=2026-08-25&to=2026-11-22").period?.options[1]?.current).toBe(true);
  });

  it("opens on every game where the page's default is all time (the games list)", () => {
    const m = model("", { defaultPeriod: "all" });
    expect(m.period?.options[4]).toMatchObject({ current: true, href: "/app" });
    expect(m.period?.options[1]?.href).toBe("/app?from=2026-08-25&to=2026-11-22");
  });

  it("opens the custom form with native dates for a range that is no preset", () => {
    const m = model("from=2026-10-01&to=2026-10-31&color=black");
    expect(m.period?.custom).toMatchObject({
      open: true,
      action: "/app",
      from: "2026-10-01",
      to: "2026-10-31",
    });
    // Everything but the range itself rides along as hidden inputs.
    expect(m.period?.custom.hidden).toEqual([["color", "black"]]);
    expect(m.period?.options.some((o) => o.current)).toBe(false);
  });

  it("is absent where the page has no dates", () => {
    expect(model("", { dates: false }).period).toBeNull();
  });
});

describe("thin window note", () => {
  it("says so and offers All time when the 90 days hold fewer than 10 games", () => {
    expect(model("", { windowGames: 4 }).thin).toEqual({
      text: "4 games in the last 90 days. Most numbers here need at least 10.",
      allTimeHref: "/app?from=all",
    });
    expect(model("", { windowGames: 1 }).thin?.text).toBe(
      "1 game in the last 90 days. Most numbers here need at least 10.",
    );
  });

  it("stays quiet with enough games, on a chosen range, or without a count", () => {
    expect(model("", { windowGames: 10 }).thin).toBeNull();
    expect(model("from=2026-10-01&to=2026-10-31", { windowGames: 2 }).thin).toBeNull();
    expect(model("from=all", { windowGames: 2 }).thin).toBeNull();
    expect(model("").thin).toBeNull();
    expect(model("", { windowGames: 0, dates: false }).thin).toBeNull();
  });
});

describe("compare periods", () => {
  it("is a toggle that turns on this month against last", () => {
    const m = model("");
    expect(m.compare).toMatchObject({ on: false, toggleHref: "/app?cmp=month" });
    expect(m.compare?.options).toEqual([]);
  });

  it("reveals A vs B presets when on, and a link that stops comparing", () => {
    const m = model("cmp=50g&color=white");
    expect(m.compare?.on).toBe(true);
    expect(m.compare?.toggleHref).toBe("/app?color=white");
    expect(m.compare?.options.map((o) => [o.label, o.current])).toEqual([
      ["This month vs last", false],
      ["Last 30 days vs the 30 before", false],
      ["Last 50 games vs the 50 before", true],
    ]);
    expect(m.compare?.custom).toMatchObject({ open: false, action: "/app" });
    expect(m.compare?.custom.hidden).toEqual([
      ["color", "white"],
      ["cmp", "custom"],
    ]);
  });

  it("fills the custom periods from the URL", () => {
    const m = model(
      "cmp=custom&a_from=2026-09-01&a_to=2026-09-30&b_from=2026-10-01&b_to=2026-10-31",
    );
    expect(m.compare?.custom).toMatchObject({
      open: true,
      a: { from: "2026-09-01", to: "2026-09-30" },
      b: { from: "2026-10-01", to: "2026-10-31" },
    });
  });

  it("hides the date presets while comparing: the periods replace the range", () => {
    expect(model("cmp=month").period).toBeNull();
    expect(model("cmp=month").periodReplaced).toBe(true);
  });

  it("is absent where the page does not compare", () => {
    expect(model("cmp=month", { compare: false }).compare).toBeNull();
  });
});

describe("summary", () => {
  it("counts the active filters and names them for the phone disclosure", () => {
    const m = model("fmt=600%2B0&color=black&from=2026-10-24&to=2026-11-22");
    expect(m.active).toBe(3);
    expect(m.summary).toBe("Rapid, 10+0, Black, last 30 days");
  });

  it("names the default 90 days without counting them, and counts All time", () => {
    expect(model("")).toMatchObject({ active: 0, summary: "Rapid, last 90 days" });
    expect(model("from=all")).toMatchObject({ active: 1, summary: "Rapid, all time" });
  });

  it("names a comparison and a custom range in words", () => {
    expect(model("cmp=month").summary).toBe("Rapid, this month vs last");
    expect(model("from=2026-10-01&to=2026-10-31").summary).toBe("Rapid, 1 Oct to 31 Oct 2026");
  });
});
