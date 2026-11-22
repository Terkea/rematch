import { describe, expect, it } from "vitest";
import { cn } from "../src/lib";
import { tokensCss } from "./design-md";

describe("cn", () => {
  it("lets the later Tailwind class win", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
  });

  it("drops falsy values", () => {
    expect(cn("a", false, undefined, "b")).toBe("a b");
  });

  it("keeps a token font size beside a text colour (text-prose is a size, not a colour)", () => {
    expect(cn("text-prose", "text-ink")).toBe("text-prose text-ink");
    expect(cn("text-ink-soft", "text-2xs")).toBe("text-ink-soft text-2xs");
  });

  it("still lets the later token size win", () => {
    expect(cn("text-prose", "text-sm")).toBe("text-sm");
  });

  it("knows every --text-* size token in tokens.css", () => {
    const css = tokensCss;
    const sizes = [...css.matchAll(/^\s*--text-([a-z0-9]+):/gm)].map((m) => m[1]);
    expect(sizes.length).toBeGreaterThan(5);
    for (const size of sizes)
      expect(cn(`text-${size}`, "text-ink"), size).toBe(`text-${size} text-ink`);
  });
});
