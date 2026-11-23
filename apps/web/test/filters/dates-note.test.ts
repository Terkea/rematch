import { describe, expect, it } from "vitest";
import { datesNote, reportWindowNote } from "../../src/app/(app)/app/_filters/dates-note";

// Audit A6: on Weaknesses and Areas the dates only trim the weekly lines. The page says so,
// and says what stays on the estimate's own window.

describe("datesNote", () => {
  it("says nothing without dates", () => {
    expect(datesNote({ from: null, to: null }, "weaknesses")).toBeNull();
  });

  it("names the dates and what they change on Weaknesses", () => {
    expect(datesNote({ from: "2026-09-01", to: "2026-10-31" }, "weaknesses")).toBe(
      "1 Sep to 31 Oct 2026: these dates trim the weekly lines only. Each rate, its range, its state and the comparison stay on your games in the last 6 months.",
    );
  });

  it("names what stays on its window on an area", () => {
    expect(datesNote({ from: "2026-09-01", to: null }, "area")).toBe(
      "From 1 Sep 2026: these dates trim the weekly lines only. The score and each measure's current value stay on your last 50 games, and the weakness rates on your games in the last 6 months.",
    );
  });

  it("always says the report's window: its text is all your games, whatever the dates", () => {
    expect(reportWindowNote({ from: null, to: null })).toBe(
      "The report is written from your whole profile, on all your games of this time control: the weakness rates cover the last 6 months.",
    );
    expect(reportWindowNote({ from: "2026-09-01", to: "2026-10-31" })).toBe(
      "1 Sep to 31 Oct 2026: these dates trim the weekly lines only. The report is written from your whole profile, on all your games of this time control: the weakness rates cover the last 6 months.",
    );
  });
});
