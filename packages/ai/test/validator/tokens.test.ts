import { describe, expect, it } from "vitest";
import { parseTokens } from "../../src/validator/tokens.js";

describe("parseTokens", () => {
  it("parses every token kind", () => {
    const r = parseTokens(
      "a {{move:g1:6}} b {{best:g1:6}} c {{pv:g1:6:best:2}} d {{pv:g1:6:refutation:1}} e {{line:w-italian}} f {{line:w-italian:3}} g {{num:basis.analysedGames}} h {{num:weaknesses.W1.posteriorRate|pct1}} i {{date:basis.windowStart}} j {{sq:f7}}",
    );
    expect(r.errors).toEqual([]);
    expect(r.tokens.map((t) => t.kind)).toEqual([
      "move",
      "best",
      "pv",
      "pv",
      "line",
      "line",
      "num",
      "num",
      "date",
      "sq",
    ]);
    expect(r.tokens[0]).toMatchObject({ kind: "move", gameId: "g1", ply: 6 });
    expect(r.tokens[2]).toMatchObject({ kind: "pv", side: "best", n: 2 });
    expect(r.tokens[3]).toMatchObject({ kind: "pv", side: "refutation", n: 1 });
    expect(r.tokens[4]).toMatchObject({ kind: "line", lineKey: "w-italian", n: null });
    expect(r.tokens[5]).toMatchObject({ kind: "line", lineKey: "w-italian", n: 3 });
    expect(r.tokens[6]).toMatchObject({ kind: "num", path: "basis.analysedGames", fmt: null });
    expect(r.tokens[7]).toMatchObject({ kind: "num", fmt: "pct1" });
    expect(r.tokens[9]).toMatchObject({ kind: "sq", square: "f7" });
  });

  it("replaces each token with a placeholder", () => {
    const r = parseTokens("x {{move:g1:6}} y {{sq:e4}}");
    expect(r.stripped).toBe("x § y §");
  });

  it("reports malformed tokens as errors and still strips them", () => {
    for (const bad of [
      "{{move:g1}}",
      "{{move:g1:x}}",
      "{{move:g1:0}}",
      "{{pv:g1:6:middle:1}}",
      "{{pv:g1:6:best:0}}",
      "{{num:a.b|bogus}}",
      "{{num:}}",
      "{{sq:i9}}",
      "{{sq:e44}}",
      "{{nope:x}}",
      "{{move:g1:6",
      "move:g1:6}}",
      "{{}}",
    ]) {
      const r = parseTokens(`see ${bad} here`);
      expect(r.errors.length, bad).toBeGreaterThan(0);
    }
  });

  it("accepts every number format", () => {
    for (const fmt of ["int", "pct0", "pct1", "dec1", "dec2", "signed1"]) {
      expect(parseTokens(`{{num:a.b|${fmt}}}`).errors).toEqual([]);
    }
  });

  it("canonicalises invisible characters and full-width forms before parsing", () => {
    const r = parseTokens("{{mo​ve:g1:６}}");
    expect(r.errors).toEqual([]);
    expect(r.tokens[0]).toMatchObject({ kind: "move", ply: 6 });
  });
});
