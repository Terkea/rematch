import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  arrivalTime,
  dampingRatio,
  dur,
  ease,
  overshoot,
  type PieceType,
  pieceWeight,
  settleFor,
  spring,
  springProgress,
  stagger,
  useReducedMotion,
} from "../src/motion";
import { bezier, code, css, ms, tableAfter } from "./design-md";

const camel = (kebab: string) => kebab.replace(/-(\w)/g, (_, c: string) => c.toUpperCase());

/** "`{ stiffness: 452, damping: 39.2 }`" to numbers. */
function springValues(cell: string | undefined) {
  return {
    stiffness: Number(/stiffness: ([\d.]+)/.exec(cell ?? "")?.[1]),
    damping: Number(/damping: ([\d.]+)/.exec(cell ?? "")?.[1]),
  };
}

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
      const { stiffness, damping } = springValues(values);
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
    (cell ?? "").split(",").map((p) => p.trim().toLowerCase()) as PieceType[];

  it("has a weight for all six pieces", () => {
    expect(masses).toHaveLength(4);
    expect(Object.keys(pieceWeight).sort()).toEqual(masses.flatMap(([p]) => pieces(p)).sort());
  });

  for (const [pieceCell, massFactor, values, zeta, arrival, lift, squash] of masses) {
    for (const piece of pieces(pieceCell)) {
      it(`${piece}: mass factor ${massFactor}, ${code(values)}, ζ ${zeta}, 90% at ${arrival}`, () => {
        const [scaleX, scaleY] = (squash ?? "").split("/").map((n) => Number(n.trim()));
        expect(pieceWeight[piece]).toEqual({
          massFactor: Number(massFactor),
          lift: Number(lift),
          squash: { scaleX, scaleY },
        });
        const { stiffness, damping } = springValues(values);
        expect(settleFor(piece)).toEqual({ type: "spring", stiffness, damping });
        // The rule behind the row: stiffness 520 / mass factor, damping holding settle's ζ.
        expect(Math.abs(stiffness - 520 / Number(massFactor))).toBeLessThan(0.5);
        expect(dampingRatio(settleFor(piece)).toFixed(2)).toBe(zeta);
        expect(Math.abs(dampingRatio(settleFor(piece)) - dampingRatio(spring.settle))).toBeLessThan(
          0.002,
        );
        expect(Math.abs(arrivalTime(settleFor(piece), 0.9) * 1000 - ms(arrival))).toBeLessThan(1);
      });
    }
  }

  it("settles knights and bishops on the settle preset itself", () => {
    expect(settleFor("knight")).toEqual(spring.settle);
  });

  // Ordered light to heavy, as the table is.
  const ladder = masses.map(([cell]) => pieces(cell)[0] as PieceType);
  for (const [lighter, heavier] of ladder.slice(0, -1).map((p, i) => [p, ladder[i + 1]] as const)) {
    it(`a ${heavier} never overtakes a ${lighter}`, () => {
      const light = settleFor(lighter);
      const heavy = settleFor(heavier as PieceType);
      expect(arrivalTime(heavy, 0.9)).toBeGreaterThan(arrivalTime(light, 0.9));
      const reached = arrivalTime(light, 1);
      let lead = 0;
      for (let t = 0; t <= 1.5; t += 0.0005) {
        const ahead = springProgress(heavy, t) - springProgress(light, t);
        // On the way in the lighter piece is strictly ahead.
        if (t > 0 && t < reached) expect(ahead).toBeLessThan(0);
        lead = Math.max(lead, ahead);
      }
      // After the lighter one arrives, any difference is under 0.03% of the distance.
      expect(lead).toBeLessThan(0.0003);
    });
  }

  it("springProgress is the step response, and arrivalTime inverts it", () => {
    expect(springProgress(spring.settle, 0)).toBe(0);
    expect(springProgress(spring.settle, 2)).toBeCloseTo(1, 6);
    const t = arrivalTime(spring.settle, 0.5);
    expect(springProgress(spring.settle, t)).toBeCloseTo(0.5, 3);
    // Overshoot is the first peak.
    let peak = 0;
    for (let s = 0; s < 1; s += 0.0005) peak = Math.max(peak, springProgress(spring.land, s));
    expect(peak - 1).toBeCloseTo(overshoot(spring.land), 3);
  });
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
