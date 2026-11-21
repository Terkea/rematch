import { describe, expect, it } from "vitest";
import {
  arrowGeometry,
  circlePath,
  fnv1a,
  mulberry32,
  parsePath,
  roughen,
  squareCenter,
  strokeOutline,
} from "../src/annotations/index.js";

/** Drawing commands after the first moveto: one per line or curve segment. */
const segments = (d: string) => parsePath(d).filter((c) => c.type !== "M").length;

describe("squareCenter", () => {
  it("puts a1 bottom left for White and top right for Black", () => {
    expect(squareCenter("a1", "white")).toEqual({ x: 0.5, y: 7.5 });
    expect(squareCenter("a1", "black")).toEqual({ x: 7.5, y: 0.5 });
    expect(squareCenter("h8", "white")).toEqual({ x: 7.5, y: 0.5 });
  });
});

describe("arrowGeometry", () => {
  it("gives a knight move an L-shaped shaft of two straight segments", () => {
    const { shaft } = arrowGeometry("g1", "f3", "white");
    expect(segments(shaft)).toBe(2);
    expect(parsePath(shaft).every((c) => c.type === "M" || c.type === "L")).toBe(true);
  });

  it("gives any other move one segment", () => {
    expect(segments(arrowGeometry("e2", "e4", "white").shaft)).toBe(1);
    expect(segments(arrowGeometry("c1", "h6", "black").shaft)).toBe(1);
  });

  it("runs the knight's long leg first unless asked for the short one", () => {
    // g1 to f3: two up, one left. The long leg ends on g3's centre, the short one on f1's.
    const corner = (d: string) => parsePath(d)[1];
    expect(corner(arrowGeometry("g1", "f3", "white").shaft)).toMatchObject({ x: 6.5, y: 5.5 });
    expect(corner(arrowGeometry("g1", "f3", "white", { knight: "short" }).shaft)).toMatchObject({
      x: 5.5,
      y: 7.5,
    });
  });

  it("starts 0.22 from the origin and ends 0.16 short, or 0.46 short of a ringed target", () => {
    const end = (d: string) => {
      const cmds = parsePath(d);
      return cmds[cmds.length - 1] as { x: number; y: number };
    };
    const first = parsePath(arrowGeometry("e2", "e4", "white").shaft)[0];
    expect(first).toMatchObject({ x: 4.5, y: 6.5 - 0.22 });
    expect(end(arrowGeometry("e2", "e4", "white").shaft).y).toBeCloseTo(4.5 + 0.16, 6);
    expect(end(arrowGeometry("e2", "e4", "white", { ringed: true }).shaft).y).toBeCloseTo(
      4.5 + 0.46,
      6,
    );
  });

  it("draws an open head as two separate strokes of 0.27 and 0.33 from the tip", () => {
    const { shaft, head } = arrowGeometry("e2", "e4", "white");
    const tip = parsePath(shaft).at(-1) as { x: number; y: number };
    const cmds = parsePath(head);
    expect(cmds.map((c) => c.type)).toEqual(["M", "L", "M", "L"]);
    const lengths = [cmds[1], cmds[3]].map((c) => {
      const p = c as { x: number; y: number };
      return Math.hypot(p.x - tip.x, p.y - tip.y);
    });
    expect(lengths.map((l) => Number(l.toFixed(3))).sort()).toEqual([0.27, 0.33]);
  });

  it("draws a solid head for a good arrow as one closed triangle", () => {
    const { head } = arrowGeometry("e2", "e4", "white", { tone: "good" });
    expect(parsePath(head).map((c) => c.type)).toEqual(["M", "L", "L", "Z"]);
  });

  it("with a seed, bows the shaft exactly as roughen does and keeps its segments", () => {
    const plain = arrowGeometry("g1", "f3", "white");
    const rough = arrowGeometry("g1", "f3", "white", { seed: "arrow:g1f3" });
    expect(rough.shaft).toBe(roughen(plain.shaft, "arrow:g1f3"));
    expect(segments(rough.shaft)).toBe(2);
  });
});

describe("roughen", () => {
  const d = arrowGeometry("e4", "e6", "white").shaft;

  it("is deterministic for a seed and differs between seeds", () => {
    expect(roughen(d, "e4-e6")).toBe(roughen(d, "e4-e6"));
    expect(roughen(d, "e4-e5")).not.toBe(roughen(d, "e4-e6"));
  });

  it("keeps the endpoints and bows 2 to 4% of the length", () => {
    const [m, q] = parsePath(roughen(d, "e4-e6"));
    expect(m).toMatchObject(parsePath(d)[0] as object);
    expect(q?.type).toBe("Q");
    const a = m as { x: number; y: number };
    const c = q as { x: number; y: number; cx: number; cy: number };
    const len = Math.hypot(c.x - a.x, c.y - a.y);
    // The curve's mid point sits half the control point's offset from the chord.
    const mx = 0.25 * a.x + 0.5 * c.cx + 0.25 * c.x;
    const my = 0.25 * a.y + 0.5 * c.cy + 0.25 * c.y;
    const off = Math.abs((c.x - a.x) * (a.y - my) - (a.x - mx) * (c.y - a.y)) / len;
    expect(off / len).toBeGreaterThanOrEqual(0.02 - 1e-9);
    expect(off / len).toBeLessThanOrEqual(0.04 + 1e-9);
  });

  it("scales the bow with amount, and amount 0 leaves the line straight", () => {
    const flat = parsePath(roughen(d, "e4-e6", 0))[1] as { cx: number; x: number };
    expect(flat.cx).toBeCloseTo(4.5, 6);
  });
});

describe("seeding (DESIGN.md, 'Determinism')", () => {
  it("hashes with FNV-1a 32-bit", () => {
    expect(fnv1a("")).toBe(0x811c9dc5);
    expect(fnv1a("a")).toBe(0xe40c292c);
    expect(fnv1a("foobar")).toBe(0xbf9cf968);
  });

  it("draws the same sequence in [0, 1) for the same key", () => {
    const a = mulberry32(fnv1a("ring:b5"));
    const b = mulberry32(fnv1a("ring:b5"));
    const xs = [a(), a(), a(), a()];
    expect(xs).toEqual([b(), b(), b(), b()]);
    for (const x of xs) expect(x >= 0 && x < 1).toBe(true);
  });
});

describe("circlePath", () => {
  const pts = (d: string) =>
    parsePath(d).filter((c) => c.type !== "Z") as { x: number; y: number }[];

  it("starts at about 1 o'clock and sweeps 380 degrees", () => {
    const p = pts(circlePath("e4", "white", "ring:e4"));
    const c = squareCenter("e4", "white");
    const angle = (q: { x: number; y: number }) =>
      (Math.atan2(q.y - c.y, q.x - c.x) * 180) / Math.PI;
    const start = angle(p[0] as { x: number; y: number });
    // −60° from three o'clock (y grows downwards), give or take the seeded tilt of 8–12°.
    expect(start).toBeGreaterThan(-75);
    expect(start).toBeLessThan(-45);
    let swept = 0;
    for (let i = 1; i < p.length; i++) {
      let step = angle(p[i] as never) - angle(p[i - 1] as never);
      if (step > 180) step -= 360;
      if (step < -180) step += 360;
      swept += step;
    }
    expect(Math.abs(swept)).toBeGreaterThan(375);
    expect(Math.abs(swept)).toBeLessThan(385);
  });

  it("ends outside its start (a spiral, not a closed ring)", () => {
    const p = pts(circlePath("e4", "white", "ring:e4"));
    const c = squareCenter("e4", "white");
    const r = (q: { x: number; y: number }) => Math.hypot(q.x - c.x, q.y - c.y);
    // At 360° the spiral has grown 2.5px of a 56px square; at 380° a little more.
    const at360 = p[Math.round(((p.length - 1) * 360) / 380)] as { x: number; y: number };
    expect(r(at360) - r(p[0] as { x: number; y: number })).toBeCloseTo(2.5 / 56, 2);
  });

  it("is deterministic and seeded", () => {
    expect(circlePath("e4", "white", "ring:e4")).toBe(circlePath("e4", "white", "ring:e4"));
    expect(circlePath("e4", "white", "ring:e5")).not.toBe(circlePath("e4", "white", "ring:e4"));
  });
});

describe("strokeOutline", () => {
  it("is a closed filled outline with round caps", () => {
    const d = strokeOutline(arrowGeometry("e2", "e4", "white").shaft, 1);
    const cmds = parsePath(d);
    expect(cmds[0]?.type).toBe("M");
    expect(cmds.filter((c) => c.type === "A")).toHaveLength(2);
    expect(cmds.at(-1)?.type).toBe("Z");
  });
});
