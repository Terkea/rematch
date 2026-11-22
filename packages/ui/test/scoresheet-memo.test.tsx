import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ScoresheetMove } from "../src/review/index.js";

// Counts renders through `cn`, which every row and every move cell calls with its own base classes
// on each render: a re-rendered row or cell is a counted call.
const renders = vi.hoisted(() => ({ rows: 0, cells: 0 }));
vi.mock("../src/lib/index.js", async (importOriginal) => {
  const real = await importOriginal<typeof import("../src/lib/index.js")>();
  return {
    ...real,
    cn: (...args: Parameters<typeof real.cn>) => {
      const first = typeof args[0] === "string" ? args[0] : "";
      if (first.includes("grid-cols-subgrid") && first.includes("border-b")) renders.rows++;
      if (first.includes("text-left")) renders.cells++;
      return real.cn(...args);
    },
  };
});

const { Scoresheet } = await import("../src/review/index.js");

afterEach(cleanup);

/** 240 synthetic plies, 120 rows (allowed in tests, B15 covers UI). */
const moves: ScoresheetMove[] = Array.from({ length: 240 }, (_, i) => ({
  ply: i + 1,
  color: i % 2 === 0 ? "white" : "black",
  san: i % 2 === 0 ? "Nf3" : "Nf6",
  judgement: i === 40 ? "blunder" : null,
  best: false,
  mine: i % 2 === 0,
}));

describe("Scoresheet memoisation", () => {
  it("renders every row once on mount", () => {
    renders.rows = 0;
    renders.cells = 0;
    render(<Scoresheet moves={moves} currentPly={61} onSelectPly={() => {}} label="Moves" />);
    expect(renders.rows).toBe(120);
    expect(renders.cells).toBe(240);
  });

  it("re-renders only the two rows and two cells a step touches", () => {
    const onSelect = () => {};
    const { rerender } = render(
      <Scoresheet moves={moves} currentPly={61} onSelectPly={onSelect} label="Moves" />,
    );
    renders.rows = 0;
    renders.cells = 0;
    rerender(<Scoresheet moves={moves} currentPly={63} onSelectPly={onSelect} label="Moves" />);
    expect(renders.rows).toBe(2);
    expect(renders.cells).toBe(2);
  });

  it("re-renders nothing in the rows for a new onSelectPly", () => {
    const { rerender } = render(
      <Scoresheet moves={moves} currentPly={61} onSelectPly={() => {}} label="Moves" />,
    );
    renders.rows = 0;
    rerender(<Scoresheet moves={moves} currentPly={61} onSelectPly={() => {}} label="Moves" />);
    expect(renders.rows).toBe(0);
  });
});
