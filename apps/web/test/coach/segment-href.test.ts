import { games } from "@ci/ui/fixtures";
import { describe, expect, it } from "vitest";
import { reviewHref, segmentGameIds, segmentHref } from "../../src/lib/segment-href.js";

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

describe("reviewHref", () => {
  // Segments carry the PLATFORM game id; the review takes `games.id` (T7 hand-off).
  const ids = { lichessAbc: "00000000-0000-4000-8000-000000000001" };
  const href = reviewHref(ids);

  it("a move links to the review by games.id", () => {
    expect(href({ kind: "move", gameId: "lichessAbc", ply: 5, san: "Nf3" })).toBe(
      "/app/games/00000000-0000-4000-8000-000000000001?ply=5",
    );
    expect(href({ kind: "best", gameId: "lichessAbc", ply: 6, san: "e5" })).toBe(
      "/app/games/00000000-0000-4000-8000-000000000001?ply=6",
    );
  });

  it("a game the map does not know links nowhere, never to a 404", () => {
    expect(href({ kind: "move", gameId: "someoneElse", ply: 5, san: "Nf3" })).toBeNull();
    expect(href({ kind: "move", gameId: "toString", ply: 5, san: "Nf3" })).toBeNull();
    expect(href({ kind: "text", text: "hi" })).toBeNull();
  });

  it("segmentGameIds lists the platform ids that link", () => {
    expect(
      segmentGameIds([
        { kind: "text", text: "a" },
        { kind: "move", gameId: "g1", ply: 1, san: "e4" },
        { kind: "pv", gameId: "g2", ply: 2, side: "best", n: 1, san: "e5" },
        { kind: "best", gameId: "g1", ply: 3, san: "Nf3" },
      ]),
    ).toEqual(["g1", "g2"]);
    expect(segmentGameIds(null)).toEqual([]);
  });
});
