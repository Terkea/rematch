import { describe, expect, it } from "vitest";
import { getStockfishVersion } from "../src/stockfish-version.js";

const inDocker = process.env.IN_DOCKER === "1";
const stockfishPath = process.env.STOCKFISH_PATH;

// Inside the container the stack must be present, so a missing binary fails.
// Outside the container the test is skipped.
describe.skipIf(!inDocker && !stockfishPath)("stockfish", () => {
  it("is Stockfish 19", async () => {
    expect(stockfishPath, "STOCKFISH_PATH must be set in the container").toBeTruthy();
    expect(await getStockfishVersion(stockfishPath as string)).toMatch(/^Stockfish 19\b/);
  });
});
