import "@testing-library/jest-dom/vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KpiStat } from "../src/stats/index.js";

// jsdom never upgrades NumberFlow's custom element, so a re-render with a new value throws inside
// its snapshot hook. These tests are about the tick, so NumberFlow becomes its static text.
vi.mock("@number-flow/react", () => ({
  default: ({ value }: { value: number }) => <span>{value}</span>,
}));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stubReducedMotion(reduce: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: reduce && query.includes("reduce"),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

const tickX = (container: HTMLElement) =>
  container.querySelector("[data-part='value']")?.getAttribute("x1");

const base = {
  label: "Rating-adjusted score",
  value: 64.2,
  format: "decimal" as const,
  interval: { low: 61, high: 67.5, level: 0.8 as const },
  basis: { games: 38, timeClass: "rapid" },
};

describe("KpiStat value tick", () => {
  it("moves the value tick with the number's roll, not ahead of it", () => {
    stubReducedMotion(false);
    const { container, rerender } = render(<KpiStat {...base} />);
    const before = tickX(container);
    rerender(<KpiStat {...base} value={66} />);
    // The roll has only just started: the tick is still where the old number was.
    expect(tickX(container)).toBe(before);
  });

  it("jumps the value tick to the new value under reduced motion", () => {
    stubReducedMotion(true);
    const { container, rerender } = render(<KpiStat {...base} />);
    rerender(<KpiStat {...base} value={66} />);
    // Domain 0 to 80: 66 sits at 82.5%.
    expect(tickX(container)).toBe("82.50%");
  });

  it("brings the value tick to the new value when the roll ends", async () => {
    stubReducedMotion(false);
    const { container, rerender } = render(<KpiStat {...base} />);
    rerender(<KpiStat {...base} value={66} />);
    await waitFor(() => expect(tickX(container)).toBe("82.50%"), { timeout: 2000 });
  });
});
