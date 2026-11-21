import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EvalBar } from "../src/board/index.js";

function mockReducedMotion(reduce: boolean) {
  vi.stubGlobal(
    "matchMedia",
    (query: string): MediaQueryList =>
      ({
        matches: reduce && query.includes("reduce"),
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) as MediaQueryList,
  );
}

const share = () => Number(screen.getByTestId("eval-fill").style.getPropertyValue("--eval-share"));
const wait = (ms: number) => act(() => new Promise((r) => setTimeout(r, ms)));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("EvalBar", () => {
  it("fills to White's win percentage: 50% at an even position", () => {
    mockReducedMotion(false);
    render(<EvalBar score={{ cp: 0 }} label="Evaluation" />);
    expect(share()).toBe(0.5);
    const meter = screen.getByRole("meter", { name: "Evaluation" });
    expect(meter).toHaveAttribute("aria-valuenow", "50");
    expect(meter).toHaveAttribute("aria-valuetext", "0.0, equal");
  });

  it("uses the win-chance scale for White's share", () => {
    mockReducedMotion(true);
    render(<EvalBar score={{ cp: 300 }} label="Evaluation" />);
    // winPercent(+300) = 50 + 50 * (2 / (1 + exp(-0.00368208 * 300)) - 1) = 75.11
    expect(share()).toBeCloseTo(0.7511, 3);
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuetext", "+3.0, White is better");
  });

  it("sets the new fill at once under reduced motion (no spring)", async () => {
    mockReducedMotion(true);
    const { rerender } = render(<EvalBar score={{ cp: 0 }} label="Evaluation" />);
    rerender(<EvalBar score={{ mate: 2 }} label="Evaluation" />);
    await wait(30);
    expect(share()).toBe(1);
  });

  it("swings to the new fill on a spring with full motion", async () => {
    mockReducedMotion(false);
    const { rerender } = render(<EvalBar score={{ cp: 0 }} label="Evaluation" />);
    rerender(<EvalBar score={{ mate: 2 }} label="Evaluation" />);
    await wait(30);
    // 30ms into a 260/20 spring the fill is on its way, not there.
    expect(share()).toBeGreaterThan(0.5);
    expect(share()).toBeLessThan(0.95);
  });

  it("maps a forced mate to the end of the bar, as on the eval graph", () => {
    mockReducedMotion(true);
    const { rerender } = render(<EvalBar score={{ mate: 3 }} label="Evaluation" />);
    expect(share()).toBe(1);
    rerender(<EvalBar score={{ mate: -1 }} label="Evaluation" />);
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuenow", "0");
  });

  it("puts the label at the winning side's end and flips with the board", () => {
    mockReducedMotion(true);
    const { rerender } = render(<EvalBar score={{ cp: -570 }} label="Evaluation" />);
    const bar = screen.getByRole("meter");
    expect(bar).toHaveAttribute("data-orientation", "white");
    expect(bar).toHaveAttribute("data-winning", "black");
    expect(screen.getAllByText("−5.7").length).toBeGreaterThan(0);
    rerender(<EvalBar score={{ cp: -570 }} orientation="black" label="Evaluation" />);
    expect(bar).toHaveAttribute("data-orientation", "black");
  });
});

describe("EvalBar mate on the board", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("gives { mate: 0 } to the side that is not to move", async () => {
    mockReducedMotion(true);
    // Fool's mate: White to move and mated.
    const mated = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    const { rerender } = render(<EvalBar score={{ mate: 0 }} fen={mated} label="Evaluation" />);
    expect(share()).toBe(0);
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuetext", "Checkmate, Black wins");
    rerender(<EvalBar score={{ mate: 0 }} fen={mated.replace(" w ", " b ")} label="Evaluation" />);
    await wait(30);
    expect(share()).toBe(1);
  });
});
