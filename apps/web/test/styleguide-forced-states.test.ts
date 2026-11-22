import { describe, expect, it } from "vitest";
import {
  forcedStateCss,
  forceSelector,
  type RuleLike,
} from "../src/app/styleguide/forced-states.js";

const style = (selectorText: string, cssText: string, cssRules?: RuleLike[]): RuleLike => ({
  selectorText,
  style: { cssText },
  ...(cssRules ? { cssRules } : {}),
});
const media = (mediaText: string, cssRules: RuleLike[]): RuleLike => ({
  media: { mediaText },
  conditionText: mediaText,
  cssRules,
});
const nested = (cssText: string): RuleLike => ({ style: { cssText } });

describe("forceSelector", () => {
  it("swaps the pseudo-class for the force attribute on the same element", () => {
    expect(forceSelector(".hover\\:bg-well:hover", "hover")).toBe(
      '.hover\\:bg-well[data-force~="hover"]',
    );
  });

  it("keeps only the parts of a selector list that use the pseudo-class", () => {
    expect(forceSelector(".a, .b:hover, .c", "hover")).toBe('.b[data-force~="hover"]');
    expect(forceSelector(".a, .c", "hover")).toBeNull();
  });

  it("does not split a list inside parentheses", () => {
    expect(forceSelector(".x:is(.a, .b):hover", "hover")).toBe(
      '.x:is(.a, .b)[data-force~="hover"]',
    );
  });

  it("tells focus-visible from focus", () => {
    expect(forceSelector(".a:focus", "focus-visible")).toBeNull();
    expect(forceSelector(".a:focus-within", "focus-visible")).toBeNull();
    expect(forceSelector(".a:focus-visible", "focus-visible")).toBe(
      '.a[data-force~="focus-visible"]',
    );
    expect(forceSelector(".a:hover", "focus-visible")).toBeNull();
  });

  it("does not take :hover for a longer name", () => {
    expect(forceSelector(".a:hovered", "hover")).toBeNull();
  });
});

describe("forcedStateCss", () => {
  it("copies a flat hover rule and drops (hover: hover), which a touch screenshot lacks", () => {
    const css = forcedStateCss(
      [
        style(".plain", "color: red;"),
        media("(hover: hover)", [style(".h\\:x:hover", "background-color: var(--surface-hover);")]),
      ],
      "hover",
    );
    expect(css).toBe('.h\\:x[data-force~="hover"]{background-color: var(--surface-hover);}');
  });

  it("keeps any other media condition around the copied rule", () => {
    const css = forcedStateCss(
      [media("(width >= 48rem)", [style(".md\\:h:hover", "color: blue;")])],
      "hover",
    );
    expect(css).toBe('@media (width >= 48rem){.md\\:h[data-force~="hover"]{color: blue;}}');
  });

  it("resolves nesting, as Tailwind v4 writes &:hover { @media (hover: hover) { … } }", () => {
    const css = forcedStateCss(
      [
        style(".hover\\:bg-ink-soft", "", [
          style("&:hover", "", [media("(hover: hover)", [nested("background-color: navy;")])]),
        ]),
      ],
      "hover",
    );
    expect(css).toBe(':is(.hover\\:bg-ink-soft)[data-force~="hover"]{background-color: navy;}');
  });

  it("keeps cascade layers so forced states sort like the real ones", () => {
    const css = forcedStateCss(
      [
        {
          name: "utilities",
          cssRules: [style(".f\\:o:focus-visible", "outline-style: solid;")],
        },
      ],
      "focus-visible",
    );
    expect(css).toBe(
      '@layer utilities{.f\\:o[data-force~="focus-visible"]{outline-style: solid;}}',
    );
  });

  it("returns nothing when no rule uses the pseudo-class", () => {
    expect(forcedStateCss([style(".a", "color: red;")], "hover")).toBe("");
  });
});
