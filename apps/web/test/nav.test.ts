import { describe, expect, it } from "vitest";
import { isCurrent, navItems, shownNavItems, withFilters } from "../src/app/(app)/_shell/nav.js";

// The header's sections (M5b Task 11): the coach pages sit after "Games", before "Accounts".

describe("navItems", () => {
  it("lists the sections in header order, the coach pages after Games", () => {
    expect(navItems.map((i) => [i.label, i.href])).toEqual([
      ["Overview", "/app"],
      ["Games", "/app/games"],
      ["Report", "/app/report"],
      ["Weaknesses", "/app/weaknesses"],
      ["Train", "/app/train"],
      ["Strengths", "/app/strengths"],
      ["Progress", "/app/progress"],
      ["Coach", "/app/coach"],
      ["Accounts", "/app/accounts"],
    ]);
  });

  it("gives each coach page its command-menu keywords", () => {
    const keywords = Object.fromEntries(navItems.map((i) => [i.label, [...i.keywords]]));
    expect(keywords.Report).toEqual(["coaching", "plan", "summary"]);
    expect(keywords.Weaknesses).toEqual(["mistakes", "leaks"]);
    expect(keywords.Train).toEqual(["practise", "drill", "puzzles", "missed moments"]);
    expect(keywords.Strengths).toEqual(["good", "keep doing", "best moves"]);
    expect(keywords.Progress).toEqual(["trends", "kpi", "weekly"]);
    expect(keywords.Coach).toEqual(["ask", "chat", "question"]);
  });
});

describe("isCurrent", () => {
  it("marks a weakness detail as Weaknesses", () => {
    expect(isCurrent("/app/weaknesses/W1", "/app/weaknesses")).toBe(true);
  });

  it("does not mark a sibling that only shares a prefix", () => {
    expect(isCurrent("/app/weaknessesx", "/app/weaknesses")).toBe(false);
    expect(isCurrent("/app/coach/new", "/app")).toBe(false);
  });
});

describe("shownNavItems", () => {
  it("hides Coach while the chat pages are not live, so nothing links to a missing page", () => {
    expect(shownNavItems(false).map((i) => i.label)).toEqual([
      "Overview",
      "Games",
      "Report",
      "Weaknesses",
      "Train",
      "Strengths",
      "Progress",
      "Accounts",
    ]);
    expect(shownNavItems(true)).toEqual(navItems);
  });
});

// Audit A7: moving between pages keeps the filters (account, time control, colour, dates).
describe("withFilters", () => {
  const q = new URLSearchParams(
    "account=acc-1&tc=blitz&color=black&from=2026-09-01&to=2026-10-31&cmp=month&anchor=x",
  );

  it("carries the scope, colour and dates to a stats page", () => {
    expect(withFilters("/app/weaknesses", q)).toBe(
      "/app/weaknesses?account=acc-1&tc=blitz&color=black&from=2026-09-01&to=2026-10-31",
    );
    expect(withFilters("/app/areas/time", q)).toBe(
      "/app/areas/time?account=acc-1&tc=blitz&color=black&from=2026-09-01&to=2026-10-31",
    );
  });

  it("speaks the games page's own words, with no account", () => {
    expect(withFilters("/app/games", q)).toBe(
      "/app/games?tc=blitz&colour=black&from=2026-09-01&to=2026-10-31",
    );
  });

  it("keeps only the scope (and colour for a new chat) on the coach pages", () => {
    expect(withFilters("/app/coach", q)).toBe("/app/coach?account=acc-1&tc=blitz");
    expect(withFilters("/app/coach/new", q)).toBe(
      "/app/coach/new?account=acc-1&tc=blitz&color=black",
    );
  });

  it("keeps only the scope on the train page", () => {
    expect(withFilters("/app/train", q)).toBe("/app/train?account=acc-1&tc=blitz");
    expect(withFilters("/app/train", new URLSearchParams("tc=all"))).toBe("/app/train");
  });

  it("reads the games page's colour, drops bad values and leaves other pages bare", () => {
    expect(withFilters("/app/progress", new URLSearchParams("colour=white&from=junk"))).toBe(
      "/app/progress?color=white",
    );
    expect(withFilters("/app/accounts", q)).toBe("/app/accounts");
    expect(withFilters("/app/report", null)).toBe("/app/report");
  });
});
