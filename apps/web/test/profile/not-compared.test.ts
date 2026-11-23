import { describe, expect, it } from "vitest";
import { type Benchmark, notComparedText } from "../../src/server/profile/benchmark-text.js";

// Holistic F1: a row the peer benchmark does not compare says why. Rows it leaves out by design
// (ADR 0081, Rust `is_peer_comparable`) say they can't be measured fairly from the opponents'
// side; a comparable row that is not compared is low-sample.

const peers: Benchmark = {
  source: "peers",
  platform: "chesscom",
  low: 650,
  high: 950,
  players: 58,
  games: 120,
  chances: { "W4.missed": 12 },
};

describe("notComparedText", () => {
  it("names the by-design exclusion for mirrored or unenriched rows", () => {
    // ADR 0101: W1 is compared now (refutation lines from the cached evals); W5 is compared with
    // the evaluation on the all-games side only, so a colour row says this.
    for (const key of ["W2", "W3.deep", "W5", "W6", "W7.prep", "W8.basics", "W10", "W11"]) {
      expect(notComparedText(peers, key)).toBe(
        "Not compared: this can't be measured fairly from your opponents' moves in games against you",
      );
    }
    expect(notComparedText(peers, "motif:fork")).toMatch(/can't be measured fairly/);
  });

  it("says a comparable row has too few of their chances", () => {
    for (const key of ["W1", "W4.missed", "W4.allowed", "W7.traps", "W9a", "W9b"]) {
      expect(notComparedText(peers, key)).toBe("Not compared yet: too few of their chances");
    }
  });

  it("without a peer benchmark never says players at your level", () => {
    expect(notComparedText(null, "W1")).toBe("Not compared yet: no benchmark for this account yet");
    expect(notComparedText({ source: "lichess_bands" }, "W1")).not.toMatch(/at your level/);
  });
});
