import { describe, expect, it } from "vitest";
import { brand } from "../src/brand.js";

// The product is "Rematch, chess improver" (open-source launch, ADR 0096). The page title, header
// wordmark, mail and auth copy read `brand`; package scopes (@ci/*), database and compose names
// keep "chess-improver".
describe("brand", () => {
  it("names the product Rematch", () => {
    expect(brand.name).toBe("Rematch");
    expect(brand.wordmark).toBe("Rematch");
  });
});
