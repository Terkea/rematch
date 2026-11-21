import { describe, expect, it } from "vitest";
import {
  code,
  contrast,
  css,
  type Declarations,
  designMd,
  ms,
  over,
  parseColour,
  px,
  tableAfter,
  tokensCss,
} from "./design-md";

const themes = { day: css.day, night: css.night } as const;
type Theme = keyof typeof themes;

function colour(theme: Theme, token: string) {
  const value = themes[theme].get(`--color-${token}`) ?? css.day.get(`--color-${token}`);
  if (!value) throw new Error(`tokens.css has no --color-${token} (${theme})`);
  return parseColour(value);
}

const colourRows = [...tableAfter("## Colour tokens"), ...tableAfter("Added in fix round 1")].map(
  ([token, day, night]) => ({ token: code(token), day: code(day), night: code(night) }),
);

const tokensNamed = (prefix: string, block: Declarations = css.day) =>
  [...block.keys()].filter((name) => name.startsWith(prefix)).sort();

const colourNames = (block: Declarations) => tokensNamed("--color-", block);

describe("colour tokens match DESIGN.md", () => {
  it("reads the full colour table from DESIGN.md", () => {
    expect(colourRows.length).toBe(22);
  });

  for (const { token, day, night } of colourRows) {
    it(`${token} day ${day}, night ${night}`, () => {
      // Compared as colours, so the formatter may change hex case or drop a trailing zero.
      expect(parseColour(css.day.get(token) ?? "")).toEqual(parseColour(day));
      expect(parseColour(css.night.get(token) ?? "")).toEqual(parseColour(night));
      expect(parseColour(css.dayScope.get(token) ?? "")).toEqual(parseColour(day));
    });
  }

  it("defines no colour tokens beyond DESIGN.md's", () => {
    expect(colourNames(css.day)).toEqual(colourRows.map((row) => row.token).sort());
  });

  it("both themes define the same set of names", () => {
    expect(colourNames(css.night)).toEqual(colourNames(css.day));
    expect(colourNames(css.dayScope)).toEqual(colourNames(css.day));
    // Every theme-dependent token (the shadow too) is redefined for night and for a day scope.
    expect([...css.night.keys()].sort()).toEqual([...css.dayScope.keys()].sort());
  });

  it("uses no #0B0B0B or #111111 (B5)", () => {
    expect(tokensCss).not.toMatch(/#0b0b0b\b|#111111\b|#111\b/i);
  });
});

describe("WCAG contrast in both themes", () => {
  const text = ["ink", "ink-soft", "ink-faint", "pen-text", "good-text"];
  const surfaces = ["paper", "sheet", "well"];
  const strokes = ["pen", "good", "pencil", "rule-strong"];

  for (const theme of ["day", "night"] as const) {
    for (const fg of text) {
      for (const bg of surfaces) {
        it(`${theme}: ${fg} on ${bg} is at least 4.5:1`, () => {
          expect(contrast(colour(theme, fg), colour(theme, bg))).toBeGreaterThanOrEqual(4.5);
        });
      }
    }
    for (const fg of strokes) {
      it(`${theme}: ${fg} on sheet is at least 3:1`, () => {
        expect(contrast(colour(theme, fg), colour(theme, "sheet"))).toBeGreaterThanOrEqual(3);
      });
    }
    it(`${theme}: pen on its halo (sheet) is at least 3:1`, () => {
      expect(contrast(colour(theme, "pen"), colour(theme, "sheet"))).toBeGreaterThanOrEqual(3);
    });
    it(`${theme}: on-ink on ink is at least 4.5:1`, () => {
      expect(contrast(colour(theme, "on-ink"), colour(theme, "ink"))).toBeGreaterThanOrEqual(4.5);
    });
  }

  // Every ratio DESIGN.md publishes is recomputed from tokens.css, so the table can't go stale.
  const contrastRows = tableAfter("### Contrast results").filter(
    (row) => !row[0]?.startsWith("**"),
  );
  it("reads the contrast table from DESIGN.md", () => {
    expect(contrastRows.length).toBeGreaterThan(40);
  });
  for (const [fgCell, bgCell, use, dayCell, nightCell, required] of contrastRows) {
    const fg = code(fgCell);
    const bg = code(bgCell);
    const selection = bgCell?.startsWith("selection on");
    for (const [theme, stated] of [
      ["day", dayCell],
      ["night", nightCell],
    ] as const) {
      if (stated === "n/a") continue;
      it(`${theme}: ${fg} on ${bgCell} (${use}) is ${stated}, at least ${required}`, () => {
        const surface = colour(theme, bg);
        const background = selection ? over(colour(theme, "selection"), surface) : surface;
        const ratio = contrast(colour(theme, fg), background);
        expect(Math.abs(ratio - Number(stated))).toBeLessThanOrEqual(0.011);
        expect(ratio).toBeGreaterThanOrEqual(Number.parseFloat(required ?? ""));
      });
    }
  }
});

describe("type, shape and space match DESIGN.md", () => {
  const steps = tableAfter("### Scale and optical size");

  it("reads all eleven type steps, 2xs to 5xl", () => {
    expect(steps.map((row) => code(row[0]))).toEqual([
      "2xs",
      "xs",
      "sm",
      "base",
      "prose",
      "lg",
      "xl",
      "2xl",
      "3xl",
      "4xl",
      "5xl",
    ]);
  });

  for (const [stepCell, sizeCell, opszCell] of steps) {
    const step = code(stepCell);
    const [size, lineHeight] = (sizeCell ?? "").split("/").map(Number);
    // The opsz column is checked against the shipped font in apps/web/test/fonts.test.ts.
    void opszCell;
    it(`text-${step} is ${size}/${lineHeight}`, () => {
      expect(px(css.day.get(`--text-${step}`) ?? "")).toBe(size);
      expect(px(css.day.get(`--text-${step}--line-height`) ?? "")).toBe(lineHeight);
    });
  }

  it("defines exactly DESIGN.md's type steps", () => {
    const sizes = [...css.day.keys()].filter((n) => /^--text-[^-]+$/.test(n)).sort();
    expect(sizes).toEqual(steps.map(([step]) => `--text-${code(step)}`).sort());
  });

  const radii = tableAfter("**Radii by role.**");
  it("has the four radii and no others", () => {
    expect(radii).toHaveLength(4);
    expect(tokensNamed("--radius-")).toEqual(radii.map(([token]) => code(token)).sort());
  });
  for (const [tokenCell, value] of radii) {
    it(`${code(tokenCell)} is ${value}`, () => {
      expect(css.day.get(code(tokenCell))).toBe(value);
    });
  }

  // "20px (16px below 480px)" becomes [[0, 16], [480, 20]].
  function responsive(description: string): [number, number][] {
    const steps: [number, number][] = [];
    const below = /(\d+)px(?:\))? below (\d+)px/.exec(description);
    const head = /^(\d+)px/.exec(description);
    if (below && head && description.includes("(")) {
      steps.push([0, Number(below[1])], [Number(below[2]), Number(head[1])]);
      return steps;
    }
    if (below) steps.push([0, Number(below[1])]);
    for (const match of description.matchAll(/(\d+)px from (\d+)px/g)) {
      steps.push([Number(match[2]), Number(match[1])]);
    }
    if (steps.length === 0 && head) steps.push([0, Number(head[1])]);
    return steps;
  }

  const spacing = tableAfter("**Spacing roles.**");
  it("has the four spacing roles and no others", () => {
    expect(spacing).toHaveLength(4);
    const roles = spacing.map(([token]) => code(token)).sort();
    expect(tokensNamed("--space-")).toEqual(roles);
    for (const block of css.media().values()) {
      for (const name of block.keys())
        if (name.startsWith("--space-")) expect(roles).toContain(name);
    }
  });
  for (const [tokenCell, description] of spacing) {
    const token = code(tokenCell);
    it(`${token} is ${description}`, () => {
      const media = css.media();
      const actual: [number, number][] = [[0, px(css.day.get(token) ?? "")]];
      for (const [width, block] of [...media.entries()].sort(([a], [b]) => a - b)) {
        const value = block.get(token);
        if (value) actual.push([width, px(value)]);
      }
      expect(actual).toEqual(responsive(description ?? ""));
    });
  }

  it("--board-size follows the board sizes table", () => {
    const rows = tableAfter("**Board sizes.**");
    expect(rows).toHaveLength(5);
    const [first, ...fixed] = rows;
    // Below 768px: the viewport less both gutters, capped, rounded down to whole 1px squares.
    const cap = /capped at (\d+)px/.exec(first?.[1] ?? "")?.[1];
    const base = (css.day.get("--board-size") ?? "").replace(/\s+/g, " ");
    expect(base).toBe(`round(down, min(100vw - 2 * var(--space-gutter), ${cap}px), 8px)`);
    const media = css.media();
    for (const [viewport, size] of fixed) {
      const from = Number(/^(\d+)/.exec(viewport ?? "")?.[1]);
      expect(media.get(from)?.get("--board-size")).toBe(size);
    }
    const overridden = [...media.values()].filter((block) => block.has("--board-size"));
    expect(overridden).toHaveLength(fixed.length);
  });

  it("--tracking-glyph is the notation glyph letter-spacing", () => {
    const value = /`letter-spacing: ([^`]+)` on the glyph/.exec(designMd)?.[1];
    expect(value).toBe("-0.01em");
    expect(css.day.get("--tracking-glyph")).toBe(value);
  });

  it("--shadow-lift matches in both themes", () => {
    const day = /`--shadow-lift: ([^`]+)`/.exec(designMd)?.[1];
    const nightColour = /\(night: `([^`]+)`\)/.exec(designMd)?.[1];
    expect(day).toBeDefined();
    expect(css.day.get("--shadow-lift")).toBe(day);
    expect(css.night.get("--shadow-lift")).toBe(day?.replace(/rgb\([^)]+\)/, nightColour ?? ""));
    expect(css.dayScope.get("--shadow-lift")).toBe(day);
  });
});

describe("motion tokens in tokens.css match DESIGN.md", () => {
  for (const [marker, prefix] of [
    ["### Duration tokens", "--dur-"],
    ["### Easing tokens", "--ease-"],
    ["### Stagger scale", "--stagger-"],
  ] as const) {
    it(`defines exactly DESIGN.md's ${prefix}* tokens`, () => {
      expect(tokensNamed(prefix)).toEqual(
        tableAfter(marker)
          .map(([token]) => code(token))
          .sort(),
      );
    });
  }

  for (const [token, value] of tableAfter("### Duration tokens")) {
    it(`${code(token)} is ${value}`, () => {
      expect(css.day.get(code(token))).toBe(`${ms(value)}ms`);
    });
  }
  for (const [token, value] of tableAfter("### Easing tokens")) {
    it(`${code(token)} is ${code(value)}`, () => {
      expect(css.day.get(code(token))).toBe(code(value));
    });
  }
  for (const [token, value] of tableAfter("### Stagger scale")) {
    it(`${code(token)} is ${value}`, () => {
      expect(css.day.get(code(token))).toBe(`${ms(value)}ms`);
    });
  }
});

describe("fonts and dark mode", () => {
  it("takes --font-serif and --font-sans from the next/font variables", () => {
    expect(css.day.get("--font-serif")).toMatch(/^var\(--font-literata\)/);
    expect(css.day.get("--font-sans")).toMatch(/^var\(--font-schibsted\)/);
    expect(css.day.get("--font-serif-italic")).toMatch(/^var\(--font-literata-italic\)/);
    expect(tokensNamed("--font-")).toEqual(["--font-sans", "--font-serif", "--font-serif-italic"]);
  });
  it("declares the data-theme dark variant", () => {
    expect(tokensCss.replaceAll('"', "")).toContain(
      "@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *));",
    );
  });
});
