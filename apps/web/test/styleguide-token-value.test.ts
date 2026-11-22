import { describe, expect, it } from "vitest";
import { tokenValue } from "../src/app/styleguide/token-value.js";

// The foundations table prints each token's computed value. A production build's CSS minifier
// rewrites `rgb(22 35 59 / 0.16)` as `#16233b29`, so the same token read differently in dev and
// production (M4 T28: the visual baselines). The table prints the source form in both.
describe("tokenValue", () => {
  it("prints a translucent hex as the rgb() the token is written in", () => {
    expect(tokenValue("#16233b29")).toBe("rgb(22 35 59 / 0.16)");
    expect(tokenValue("#56657f4d")).toBe("rgb(86 101 127 / 0.3)");
    expect(tokenValue("#e7ecf442")).toBe("rgb(231 236 244 / 0.26)");
  });

  it("rewrites the colour inside a shadow, and leaves the rest alone", () => {
    expect(tokenValue("0 10px 30px -12px #16233b59")).toBe(
      "0 10px 30px -12px rgb(22 35 59 / 0.35)",
    );
  });

  it("keeps opaque hex, a short hex and values that are already rgb()", () => {
    expect(tokenValue("#16233b")).toBe("#16233b");
    expect(tokenValue("#fff8")).toBe("rgb(255 255 255 / 0.53)");
    expect(tokenValue("rgb(22 35 59 / 0.16)")).toBe("rgb(22 35 59 / 0.16)");
    expect(tokenValue("12px")).toBe("12px");
  });
});
