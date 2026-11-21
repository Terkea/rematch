import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  dampingRatio,
  dur,
  ease,
  overshoot,
  pieceWeight,
  settleFor,
  spring,
  stagger,
  useReducedMotion,
} from "../src/motion";
import { bezier, code, css, ms, tableAfter } from "./design-md";

const camel = (kebab: string) => kebab.replace(/-(\w)/g, (_, c: string) => c.toUpperCase());

describe("motion tokens mirror DESIGN.md and tokens.css", () => {
  it("dur.draw is 0.42s", () => {
    expect(dur.draw).toBe(0.42);
  });

  const durations = tableAfter("### Duration tokens");
  it("has every duration and no others", () => {
    expect(Object.keys(dur).sort()).toEqual(
      durations.map(([token]) => camel(code(token).replace("--dur-", ""))).sort(),
    );
  });
  for (const [token, value] of durations) {
    const key = camel(code(token).replace("--dur-", "")) as keyof typeof dur;
    it(`dur.${key} is ${value} in seconds, as in tokens.css`, () => {
      expect(dur[key]).toBeCloseTo(ms(value) / 1000, 10);
      expect(css.day.get(code(token))).toBe(`${dur[key] * 1000}ms`);
    });
  }

  const easings = tableAfter("### Easing tokens");
  it("has every easing and no others", () => {
    expect(Object.keys(ease).sort()).toEqual(
      easings.map(([token]) => camel(code(token).replace("--ease-", ""))).sort(),
    );
  });
  for (const [token, value] of easings) {
    const key = camel(code(token).replace("--ease-", "")) as keyof typeof ease;
    it(`ease.${key} is ${code(value)}, as in tokens.css`, () => {
      expect([...ease[key]]).toEqual(bezier(code(value)));
      expect(bezier(css.day.get(code(token)) ?? "")).toEqual([...ease[key]]);
    });
  }

  const staggers = tableAfter("### Stagger scale");
  it("has every stagger and no others", () => {
    expect(Object.keys(stagger).sort()).toEqual(
      staggers.map(([token]) => code(token).replace("--stagger-", "")).sort(),
    );
  });
  for (const [token, value] of staggers) {
    const key = code(token).replace("--stagger-", "") as keyof typeof stagger;
    it(`stagger.${key} is ${value} in seconds, as in tokens.css`, () => {
      expect(stagger[key]).toBeCloseTo(ms(value) / 1000, 10);
      expect(css.day.get(code(token))).toBe(`${Math.round(stagger[key] * 1000)}ms`);
    });
  }
});

describe("springs match DESIGN.md", () => {
  const presets = tableAfter("### Spring presets");

  it("has every preset and no others", () => {
    expect(Object.keys(spring).sort()).toEqual(presets.map(([name]) => camel(code(name))).sort());
  });

  for (const [nameCell, values, zeta, shoot] of presets) {
    const key = camel(code(nameCell)) as keyof typeof spring;
    it(`${code(nameCell)} is ${code(values)}, ζ ${zeta}, overshoot ${shoot}`, () => {
      const stiffness = Number(/stiffness: (\d+)/.exec(values ?? "")?.[1]);
      const damping = Number(/damping: (\d+)/.exec(values ?? "")?.[1]);
      expect(spring[key]).toEqual({ type: "spring", stiffness, damping });
      expect(dampingRatio(spring[key]).toFixed(2)).toBe(zeta);
      // "24%" holds to ±0.5 points, "0.06%" to ±0.005.
      const stated = Number.parseFloat(shoot ?? "");
      const decimals = (shoot ?? "").split(".")[1]?.replace("%", "").length ?? 0;
      expect(Math.abs(overshoot(spring[key]) * 100 - stated)).toBeLessThanOrEqual(
        0.5 * 10 ** -decimals,
      );
    });
  }

  const masses = tableAfter("**Mass makes weight visible.**");
  const pieces = (cell: string | undefined) =>
    (cell ?? "").split(",").map((p) => p.trim().toLowerCase()) as (keyof typeof pieceWeight)[];

  it("has a weight for all six pieces", () => {
    expect(Object.keys(pieceWeight).sort()).toEqual(masses.flatMap(([p]) => pieces(p)).sort());
  });

  for (const [pieceCell, mass, zeta, lift, squash] of masses) {
    for (const piece of pieces(pieceCell)) {
      it(`${piece}: mass ${mass}, settle ζ ${zeta}, lift ${lift}, squash ${squash}`, () => {
        const [scaleX, scaleY] = (squash ?? "").split("/").map((n) => Number(n.trim()));
        expect(pieceWeight[piece]).toEqual({
          mass: Number(mass),
          lift: Number(lift),
          squash: { scaleX, scaleY },
        });
        expect(settleFor(piece)).toEqual({ ...spring.settle, mass: Number(mass) });
        expect(dampingRatio(settleFor(piece)).toFixed(2)).toBe(zeta);
      });
    }
  }
});

describe("useReducedMotion", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function stubMatchMedia(initial: boolean) {
    let matches = initial;
    const listeners = new Set<() => void>();
    const query = {
      get matches() {
        return matches;
      },
      media: "(prefers-reduced-motion: reduce)",
      addEventListener: (_: string, fn: () => void) => listeners.add(fn),
      removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
    };
    const matchMedia = vi.fn((media: string) => {
      expect(media).toBe("(prefers-reduced-motion: reduce)");
      return query;
    });
    vi.stubGlobal("matchMedia", matchMedia);
    return {
      set(next: boolean) {
        matches = next;
        for (const fn of listeners) fn();
      },
    };
  }

  it("returns true when prefers-reduced-motion: reduce matches", () => {
    stubMatchMedia(true);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(true);
  });

  it("returns false otherwise, and follows changes", () => {
    const media = stubMatchMedia(false);
    const { result } = renderHook(() => useReducedMotion());
    expect(result.current).toBe(false);
    act(() => media.set(true));
    expect(result.current).toBe(true);
  });
});
