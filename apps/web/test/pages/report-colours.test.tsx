import { createRequire } from "node:module";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  colourCompareText,
  colourMetricText,
  colourRecordText,
  colourWeaknessPeerText,
} from "../../src/app/(app)/app/report/_sections/colours-text.js";
import type {
  ByColor,
  ColorMetric,
  ColorSummary,
  InputBenchmark,
} from "../../src/server/coaching/types.js";

// The report's White vs Black block (ADR 0085), from the stored computed report's `byColor`.
// The numbers are a synthetic fixture (`backend/fixtures/ai/input/colour-split.v2.json`), never
// owner data.

const { Colours } = await import("../../src/app/(app)/app/report/_sections/colours.js");

const fixture = JSON.parse(
  (await import("node:fs")).readFileSync(
    new URL("../../../../backend/fixtures/ai/input/colour-split.v2.json", import.meta.url),
    "utf8",
  ),
) as { byColor: ByColor; benchmark: InputBenchmark };

type Dom = { window: { document: Document; close(): void } };
const { JSDOM } = createRequire(import.meta.url)("jsdom") as {
  JSDOM: new (html: string) => Dom;
};

function text(ui: React.ReactElement): string {
  const dom = new JSDOM(`<!doctype html><body>${renderToString(ui)}</body>`);
  const t = dom.window.document.body.textContent ?? "";
  dom.window.close();
  return t.replace(/\s+/g, " ");
}

const metric = (value: number | null, low: number | null, high: number | null): ColorMetric => ({
  value,
  unit: "rate",
  n: 30,
  low,
  high,
  sufficient: value !== null,
  needMore: value === null ? 4 : null,
});

describe("White vs Black text", () => {
  it("states the record with n, and no games as none yet", () => {
    expect(colourRecordText(fixture.byColor.white)).toBe("42 games: 20 won, 4 drawn, 18 lost");
    const none: ColorSummary = { ...fixture.byColor.black, games: 0, wins: 0, losses: 0 };
    expect(colourRecordText(none)).toBe("No analysed games yet");
  });

  it("an insufficient number is a dash and what it needs, never 0", () => {
    const black = fixture.byColor.black;
    expect(colourMetricText(black.score, "score")).toEqual({
      value: "—",
      detail: "Need 3 more games",
    });
    expect(colourMetricText(fixture.byColor.white.accuracy, "accuracy")).toEqual({
      value: "78.4%",
      detail: "80% range 76.9–79.8%",
    });
    expect(colourMetricText(fixture.byColor.white.score, "score")).toEqual({
      value: "52%",
      detail: "80% range 44–60%",
    });
    expect(colourMetricText(fixture.byColor.white.blundersPerGame, "blunders")).toEqual({
      value: "0.9",
      detail: "80% range 0.7–1.1",
    });
  });

  it("calls a difference only when the ranges separate", () => {
    expect(colourCompareText("score", metric(0.62, 0.55, 0.68), metric(0.4, 0.33, 0.47))).toBe(
      "You score more as White.",
    );
    expect(colourCompareText("blunders", metric(1.4, 1.2, 1.6), metric(0.8, 0.6, 1.0), false)).toBe(
      "You blunder less as Black.",
    );
    expect(colourCompareText("score", metric(0.52, 0.44, 0.6), metric(0.5, 0.41, 0.58))).toBe(
      "No clear difference between your White and Black games yet.",
    );
    expect(colourCompareText("score", metric(0.52, 0.44, 0.6), metric(null, null, null))).toBe(
      null,
    );
  });

  it("names a colour's own peers with the band, never players at your level", () => {
    const [w4, w1] = fixture.byColor.white.weaknesses;
    if (!w4 || !w1) throw new Error("the fixture has two White weakness rows");
    expect(colourWeaknessPeerText(w4, "white", fixture.benchmark)).toBe(
      "Players you've faced who had White, rated about 675–975: 9.6% (14 players)",
    );
    expect(colourWeaknessPeerText(w1, "white", fixture.benchmark)).toBe(
      "Not compared: no players you've faced with White to compare with yet",
    );
  });
});

describe("White vs Black block", () => {
  it("renders both colours, the shortfall and the bias", () => {
    const t = text(<Colours byColor={fixture.byColor} benchmark={fixture.benchmark} order={1} />);
    expect(t).toContain("White and Black");
    expect(t).toContain("As White");
    expect(t).toContain("42 games: 20 won, 4 drawn, 18 lost");
    expect(t).toContain("As Black");
    expect(t).toContain("7 games: 3 won, 0 drawn, 4 lost");
    expect(t).toContain("Need 3 more games with Black before comparing");
    expect(t).toContain("Missing a mate");
    expect(t).toContain("Leaving pieces hanging");
    expect(t).toContain("Only their games against you count");
    expect(t).not.toMatch(/NaN|Infinity|undefined|players at your level/);
    // Black is not sufficient: no comparison sentence is made.
    expect(t).not.toContain("No clear difference");
  });

  it("shows nothing for a report from before the split", () => {
    expect(renderToString(<Colours byColor={undefined} benchmark={undefined} order={1} />)).toBe(
      "",
    );
  });
});

describe("report strengths", () => {
  it("are the Strengths page's: linked, and an area strength reads in plain words", async () => {
    const { strengthLabel, strengthsHref } = await import(
      "../../src/app/(app)/app/report/_sections/links.js"
    );
    expect(strengthLabel({ label: "Strong area: tactics" })).toBe("A strong area");
    expect(strengthLabel({ label: "Few errors: Missed tactic" })).toBe("Few errors: Missed tactic");
    expect(strengthsHref({ accountId: "a1", timeClass: "rapid" })).toBe(
      "/app/strengths?account=a1&tc=rapid",
    );
  });
});
