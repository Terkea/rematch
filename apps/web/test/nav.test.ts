import { describe, expect, it } from "vitest";
import { isCurrent, navItems } from "../src/app/(app)/_shell/nav.js";

// The header's sections (M5b Task 11): the coach pages sit after "Games", before "Accounts".

describe("navItems", () => {
  it("lists the sections in header order, the coach pages after Games", () => {
    expect(navItems.map((i) => [i.label, i.href])).toEqual([
      ["Overview", "/app"],
      ["Games", "/app/games"],
      ["Report", "/app/report"],
      ["Weaknesses", "/app/weaknesses"],
      ["Progress", "/app/progress"],
      ["Coach", "/app/coach"],
      ["Accounts", "/app/accounts"],
    ]);
  });

  it("gives each coach page its command-menu keywords", () => {
    const keywords = Object.fromEntries(navItems.map((i) => [i.label, [...i.keywords]]));
    expect(keywords.Report).toEqual(["coaching", "plan", "summary"]);
    expect(keywords.Weaknesses).toEqual(["mistakes", "leaks"]);
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
