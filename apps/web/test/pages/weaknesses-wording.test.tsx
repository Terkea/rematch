// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  BandKey,
  groupColourWeaknesses,
} from "../../src/app/(app)/app/weaknesses/weaknesses-view.js";
import type { Benchmark } from "../../src/server/profile/benchmark-text.js";

// Q4: with a peer benchmark the colour lede and the key name the players faced, never "players at
// your level" or "your rating band".

const peers: Benchmark = {
  source: "peers",
  platform: "chesscom",
  low: 650,
  high: 950,
  players: 58,
  games: 120,
  chances: {},
};

describe("colour mode wording", () => {
  it("the colour group names the players faced", () => {
    const [group] = groupColourWeaknesses([], "White", peers);
    expect(group?.lede).toBe(
      "Compared with the players you've faced, in their White games, most pressing first. States are called on all your games, so they are not shown per colour.",
    );
    const [legacy] = groupColourWeaknesses([], "White", { source: "lichess_bands" });
    expect(legacy?.lede).toMatch(
      /^Compared with players in your rating band, in their White games/,
    );
  });

  it("the key names the players faced", () => {
    const { container } = render(<BandKey benchmark={peers} />);
    expect(container.textContent).toContain("Players you've faced, rated about 650–950, median");
    expect(container.textContent).not.toMatch(/rating band/);
  });
});
