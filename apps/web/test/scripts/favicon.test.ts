import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { faviconIco, iconSvg, markColours, readDayTokens } from "../../scripts/favicon";

// M1: `/favicon.ico` was a 404. The mark is generated from the design tokens (an ink tile holding
// a 2x2 board), so the committed files must match what the tokens give today.

const APP = join(dirname(fileURLToPath(import.meta.url)), "../../src/app");
const TOKENS = join(
  dirname(fileURLToPath(import.meta.url)),
  "../../../../packages/ui/src/styles/tokens.css",
);

describe("the favicon (M1)", () => {
  const colours = markColours(readDayTokens(readFileSync(TOKENS, "utf8")));

  it("takes its colours from the day tokens", () => {
    expect(colours).toEqual({ tile: "#1f2a3d", light: "#e6e1d8", dark: "#b8b2a6" });
  });

  it("is a real ICO with 16, 32 and 48 px PNG images", () => {
    const ico = faviconIco(colours);
    expect(ico.readUInt16LE(0)).toBe(0);
    expect(ico.readUInt16LE(2)).toBe(1);
    expect(ico.readUInt16LE(4)).toBe(3);
    const sizes = [0, 1, 2].map((i) => ico.readUInt8(6 + i * 16));
    expect(sizes).toEqual([16, 32, 48]);
    for (const i of [0, 1, 2]) {
      const offset = ico.readUInt32LE(6 + i * 16 + 12);
      expect(ico.subarray(offset, offset + 8)).toEqual(
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      );
    }
  });

  it("is committed as generated: src/app/favicon.ico and src/app/icon.svg", () => {
    expect(readFileSync(join(APP, "favicon.ico")).equals(faviconIco(colours))).toBe(true);
    expect(readFileSync(join(APP, "icon.svg"), "utf8")).toBe(iconSvg(colours));
  });
});
