import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ComputedReport } from "../../src/server/coaching/types.js";

// The report's "Where you stand" (Q7, Q8): area scores render as whole percentiles, and with a
// peer benchmark the section names the players faced, never the old Lichess rating band.

const { Snapshot } = await import("../../src/app/(app)/app/report/_sections/snapshot.js");

const fixture = JSON.parse(
  readFileSync(
    new URL("../../../../backend/fixtures/ai/payload/report-payload.example.json", import.meta.url),
    "utf8",
  ),
) as { computed: ComputedReport };

type Dom = { window: { document: Document; close(): void } };
const { JSDOM } = createRequire(import.meta.url)("jsdom") as {
  JSDOM: new (html: string) => Dom;
};

function text(computed: ComputedReport): string {
  const dom = new JSDOM(
    `<!doctype html><body>${renderToString(<Snapshot computed={computed} order={1} />)}</body>`,
  );
  const t = dom.window.document.body.textContent ?? "";
  dom.window.close();
  return t.replace(/\s+/g, " ");
}

function peers(): ComputedReport {
  const c = structuredClone(fixture.computed);
  c.player = { ...c.player, platform: "chesscom", rating: 825, band: "B1" };
  c.benchmark = { source: "peers", ratingLow: 667, ratingHigh: 967, players: 25, games: 61 };
  c.areaScores = c.areaScores.map((a, i) =>
    a.area === "openings"
      ? { ...a, score: 44.33057115152735, sufficient: true }
      : a.area === "tactics"
        ? { ...a, score: 6.309910793819363, sufficient: true }
        : { ...a, score: a.score === null ? null : a.score + i / 7 },
  );
  return c;
}

describe("Where you stand", () => {
  it("renders whole area scores, read aloud and seen", () => {
    const t = text(peers());
    expect(t).toContain("Openings 44,");
    expect(t).toContain("Tactics 6,");
    // No score, median or range with decimals anywhere in the section.
    expect(t).not.toMatch(/\d\.\d/);
  });

  it("names the players faced and what a score means against them", () => {
    const t = text(peers());
    expect(t).toContain(
      "Rated 825 in rapid on chess.com, compared with players you've faced on chess.com, rated about 667–967.",
    );
    expect(t).toContain(
      "Scores compare you with players you've faced on chess.com, rated about 667–967: 50 is level with them and higher is better.",
    );
    expect(t).not.toMatch(/players rated 900–1299|band median/);
  });

  it("keeps the rating band's words without a peer benchmark", () => {
    const t = text(fixture.computed);
    expect(t).toContain("among players rated 1300–1699");
    expect(t).toContain("Scores are percentiles against players rated 1300–1699");
  });
});
