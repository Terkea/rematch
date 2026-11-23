// The Rematch palette (docs/superpowers/specs/2026-11-23-rematch-brand-design.md): faded photo
// paper, navy ink, dad's red pen and a crayon yellow for celebrations. These checks hold the brand's
// promises that tokens.test.ts doesn't: the warm paper is never yellow, night study is a desk lamp
// on warm paper rather than navy, every text and mark keeps AA in both themes, and the crayon stays
// out of the data.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { contrast, css, parseColour, type Rgba } from "./design-md";

const themes = { day: css.day, night: css.night } as const;
type Theme = keyof typeof themes;

function colour(theme: Theme, token: string): Rgba {
  const value = themes[theme].get(`--color-${token}`) ?? css.day.get(`--color-${token}`);
  if (!value) throw new Error(`tokens.css has no --color-${token} (${theme})`);
  return parseColour(value);
}

/** OKLCH lightness, chroma and hue (degrees) of an sRGB colour. */
function oklch([r, g, b]: Rgba): { l: number; c: number; h: number } {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  const [R, G, B] = [lin(r), lin(g), lin(b)];
  const l = Math.cbrt(0.4122214708 * R + 0.5363325363 * G + 0.0514459929 * B);
  const m = Math.cbrt(0.2119034982 * R + 0.6806995451 * G + 0.1073969566 * B);
  const s = Math.cbrt(0.0883024619 * R + 0.2817188376 * G + 0.6299787005 * B);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  const h = ((Math.atan2(bb, a) * 180) / Math.PI + 360) % 360;
  return { l: L, c: Math.hypot(a, bb), h };
}

describe("the Rematch day palette", () => {
  // The spec's values. They may move only to pass contrast, and these all pass.
  const spec: [string, string][] = [
    ["paper", "#F4EFE6"],
    ["sheet", "#FBF8F2"],
    ["ink", "#1F2A3D"],
    ["pen", "#C8372D"],
    ["good", "#2F6B4F"],
    ["pencil", "#6B6458"],
    ["crayon", "#E3A92F"],
  ];
  for (const [token, value] of spec) {
    it(`${token} is ${value}`, () => {
      expect(colour("day", token)).toEqual(parseColour(value));
    });
  }
});

describe("paper is faded, warm and photographic, never yellow", () => {
  for (const theme of ["day", "night"] as const) {
    for (const surface of ["paper", "sheet", "well"]) {
      it(`${theme} ${surface} is a warm hue at low chroma`, () => {
        const { c, h } = oklch(colour(theme, surface));
        // Warm: between red-orange and yellow-green on the OKLCH wheel, never navy (about 260°).
        expect(h).toBeGreaterThanOrEqual(40);
        expect(h).toBeLessThanOrEqual(100);
        // Faded, not cream: a yellow or cream page sits well above 0.03.
        expect(c).toBeGreaterThan(0.004);
        expect(c).toBeLessThanOrEqual(0.025);
      });
    }
  }
  it("night study is a desk lamp on warm paper: dark, and not blue", () => {
    const { l, h } = oklch(colour("night", "paper"));
    expect(l).toBeLessThan(0.3);
    expect(h < 200 || h > 300).toBe(true);
  });
});

describe("AA contrast for every text and mark, day and night", () => {
  const text = ["ink", "ink-soft", "ink-faint", "pen-text", "good-text", "pencil"];
  const marks = ["ink", "pen", "good", "pencil", "rule-strong"];
  const surfaces = ["paper", "sheet", "well"];
  for (const theme of ["day", "night"] as const) {
    for (const fg of text) {
      for (const bg of surfaces) {
        it(`${theme}: ${fg} text on ${bg} is at least 4.5:1`, () => {
          expect(contrast(colour(theme, fg), colour(theme, bg))).toBeGreaterThanOrEqual(4.5);
        });
      }
    }
    for (const fg of marks) {
      for (const bg of surfaces) {
        it(`${theme}: ${fg} mark on ${bg} is at least 3:1`, () => {
          expect(contrast(colour(theme, fg), colour(theme, bg))).toBeGreaterThanOrEqual(3);
        });
      }
    }
    for (const bg of ["ink", "ink-soft", "pen", "good"]) {
      it(`${theme}: on-ink on ${bg} is at least 4.5:1`, () => {
        expect(contrast(colour(theme, "on-ink"), colour(theme, bg))).toBeGreaterThanOrEqual(4.5);
      });
    }
  }
});

describe("crayon: celebrations only, never data", () => {
  it("is defined for both themes", () => {
    expect(css.day.has("--color-crayon")).toBe(true);
    expect(css.night.has("--color-crayon")).toBe(true);
    expect(css.dayScope.has("--color-crayon")).toBe(true);
  });

  for (const theme of ["day", "night"] as const) {
    it(`${theme}: never reads as the red pen (at least 30° of hue away)`, () => {
      const pen = oklch(colour(theme, "pen")).h;
      const crayon = oklch(colour(theme, "crayon")).h;
      const apart = Math.min(Math.abs(pen - crayon), 360 - Math.abs(pen - crayon));
      expect(apart).toBeGreaterThanOrEqual(30);
    });
    it(`${theme}: shows on paper and sheet (decorative, at least 1.8:1)`, () => {
      for (const bg of ["paper", "sheet"]) {
        expect(contrast(colour(theme, "crayon"), colour(theme, bg))).toBeGreaterThanOrEqual(1.8);
      }
    });
  }

  // Charts, boards, tables and stats stay crisp: the crayon may appear only in the brand touches,
  // the styleguide that documents them, and the token file.
  it("appears only in the brand touches", () => {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
    const allowed = [
      /^packages\/ui\/src\/styles\/tokens\.css$/,
      /^packages\/ui\/src\/brand\//,
      /^apps\/web\/src\/app\/styleguide\//,
    ];
    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) {
          if (name !== "node_modules" && name !== ".next") walk(path);
        } else if (/\.(tsx?|css)$/.test(name) && readFileSync(path, "utf8").includes("crayon")) {
          const rel = relative(root, path).replace(/\\/g, "/");
          if (!allowed.some((re) => re.test(rel))) offenders.push(rel);
        }
      }
    };
    walk(join(root, "packages/ui/src"));
    walk(join(root, "apps/web/src"));
    expect(offenders).toEqual([]);
  });
});
