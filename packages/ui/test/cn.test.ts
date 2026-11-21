import { describe, expect, it } from "vitest";
import { cn } from "../src/lib";

describe("cn", () => {
  it("lets the later Tailwind class win", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
  });

  it("drops falsy values", () => {
    expect(cn("a", false, undefined, "b")).toBe("a b");
  });
});
