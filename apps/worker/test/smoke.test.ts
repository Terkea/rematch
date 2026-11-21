import { describe, expect, it } from "vitest";
import { getStockfishVersion } from "../src/stockfish-version.js";

const stockfishPath = process.env.STOCKFISH_PATH;

describe.skipIf(!stockfishPath)("stockfish", () => {
  it("is Stockfish 19", async () => {
    expect(await getStockfishVersion(stockfishPath as string)).toMatch(/^Stockfish 19\b/);
  });
});
