import { games } from "@ci/ui/fixtures";
import { describe, expect, it } from "vitest";
import { segmentHref } from "../../src/lib/segment-href.js";

describe("segmentHref", () => {
  it("a move links to the review at its ply", () => {
    // Rust `Segment::Move.ply` is the ply of the move played (1-based). M5a's review at `?ply=N`
    // shows the position after ply N with that move highlighted: the fixture's plies[N - 1].
    const fixture = games.middlegameBlunder;
    const played = fixture.plies[4];
    expect(played?.ply).toBe(5);
    const href = segmentHref({ kind: "move", gameId: "g-123", ply: 5, san: played?.san ?? "" });
    expect(href).toBe("/app/games/g-123?ply=5");
  });

  it("best and pv link to their ply too", () => {
    expect(segmentHref({ kind: "best", gameId: "g", ply: 7, san: "Nf3" })).toBe(
      "/app/games/g?ply=7",
    );
    expect(segmentHref({ kind: "pv", gameId: "g", ply: 9, side: "best", n: 1, san: "e4" })).toBe(
      "/app/games/g?ply=9",
    );
  });

  it("game ids are escaped", () => {
    expect(segmentHref({ kind: "move", gameId: "a/b?c", ply: 1, san: "e4" })).toBe(
      "/app/games/a%2Fb%3Fc?ply=1",
    );
  });

  it("other segments link to nothing", () => {
    expect(segmentHref({ kind: "line", lineKey: "e4 e5", n: 2, san: "e5" })).toBeNull();
    expect(segmentHref({ kind: "eng", evalId: "e", line: null, n: null, text: "+0.3" })).toBeNull();
    expect(segmentHref({ kind: "text", text: "hi" })).toBeNull();
    expect(segmentHref({ kind: "square", square: "e4" })).toBeNull();
  });
});
