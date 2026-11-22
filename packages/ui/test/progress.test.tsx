import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProgressPlaceholder } from "../src/progress/index.js";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const label = (c: HTMLElement) => c.querySelector("[data-part='label']")?.textContent;
const stroke = (c: HTMLElement) => c.querySelector<HTMLElement>("[data-part='stroke']");

describe("ProgressPlaceholder text", () => {
  it("states done / total and the unit when the total is known", () => {
    const { container } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 38, total: 200, unit: "rapid games" }}
      />,
    );
    expect(label(container)).toBe("Analysing 38 / 200 rapid games");
  });

  it("says 'so far' when the total is unknown", () => {
    const { container } = render(
      <ProgressPlaceholder shape="list" verb="Importing" progress={{ done: 120, unit: "games" }} />,
    );
    expect(label(container)).toBe("Importing: 120 games so far");
  });

  it("takes Writing and Building as verbs", () => {
    const { container } = render(
      <ProgressPlaceholder
        shape="list"
        verb="Writing"
        progress={{ done: 1, total: 5, unit: "sections" }}
      />,
    );
    expect(label(container)).toBe("Writing 1 / 5 sections");
    cleanup();
    const { container: b } = render(<ProgressPlaceholder shape="list" verb="Building" />);
    expect(label(b)).toBe("Building");
  });

  it("shows the verb alone without progress", () => {
    const { container } = render(<ProgressPlaceholder shape="stat" verb="Analysing" />);
    expect(label(container)).toBe("Analysing");
  });

  it("never uses a spinner", () => {
    const { container } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 1, total: 2, unit: "games" }}
      />,
    );
    expect(container.innerHTML).not.toMatch(/animate-spin/);
  });

  it("renders the basis note and exactly one skeleton per shape", () => {
    const { container, rerender } = render(
      <ProgressPlaceholder
        shape="list"
        verb="Analysing"
        basisNote="Counts update as games finish."
      />,
    );
    expect(screen.getByText("Counts update as games finish.")).toBeInTheDocument();
    for (const shape of ["graph", "stat", "list", "board"] as const) {
      rerender(<ProgressPlaceholder shape={shape} verb="Analysing" />);
      expect(container.querySelectorAll("[data-shape]")).toHaveLength(1);
      expect(container.querySelector(`[data-shape='${shape}']`)).not.toBeNull();
    }
  });
});

describe("pencil stroke", () => {
  it("grows with done / total, and only when the total is known", () => {
    const { container, rerender } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 50, total: 200, unit: "games" }}
      />,
    );
    expect(stroke(container)).toHaveAttribute("data-fraction", "0.25");
    rerender(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 150, total: 200, unit: "games" }}
      />,
    );
    expect(stroke(container)).toHaveAttribute("data-fraction", "0.75");
    rerender(
      <ProgressPlaceholder shape="graph" verb="Importing" progress={{ done: 5, unit: "games" }} />,
    );
    expect(stroke(container)).toBeNull();
  });

  it("clamps the fraction to 1 and treats a zero total as 0", () => {
    const { container, rerender } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 9, total: 4, unit: "games" }}
      />,
    );
    expect(stroke(container)).toHaveAttribute("data-fraction", "1");
    rerender(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 0, total: 0, unit: "games" }}
      />,
    );
    expect(stroke(container)).toHaveAttribute("data-fraction", "0");
  });
});

describe("reduced motion", () => {
  const media = (matches: boolean) =>
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches,
      media: query,
      addEventListener() {},
      removeEventListener() {},
      addListener() {},
      removeListener() {},
      dispatchEvent: () => false,
      onchange: null,
    }));

  it("idles with a drifting segment under full motion when the total is unknown", () => {
    media(false);
    const { container } = render(
      <ProgressPlaceholder shape="list" verb="Importing" progress={{ done: 3, unit: "games" }} />,
    );
    expect(container.querySelector("[data-part='idle']")).not.toBeNull();
  });

  it("drops the drifting segment under reduced motion", () => {
    media(true);
    const { container } = render(
      <ProgressPlaceholder shape="list" verb="Importing" progress={{ done: 3, unit: "games" }} />,
    );
    expect(container.querySelector("[data-part='idle']")).toBeNull();
  });

  it("keeps the stroke at its final scale on the first paint", () => {
    media(true);
    const { container } = render(
      <ProgressPlaceholder
        shape="list"
        verb="Analysing"
        progress={{ done: 50, total: 200, unit: "games" }}
      />,
    );
    expect(stroke(container)).toHaveAttribute("data-fraction", "0.25");
  });
});

describe("live region", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  const props = (done: number, total = 200) => ({
    shape: "graph" as const,
    verb: "Analysing" as const,
    progress: { done, total, unit: "games" },
  });

  it("is one short, visually hidden polite status, apart from the visible label", () => {
    const { container } = render(<ProgressPlaceholder {...props(0)} />);
    const statuses = screen.getAllByRole("status");
    expect(statuses).toHaveLength(1);
    const status = statuses[0] as HTMLElement;
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveClass("sr-only");
    expect(status).toHaveTextContent("Analysing started");
    expect(container.querySelector("[data-part='label']")).not.toContainElement(status);
  });

  it("announces state changes at once: started, counting, done", () => {
    const { rerender } = render(<ProgressPlaceholder {...props(0)} />);
    expect(screen.getByRole("status")).toHaveTextContent("Analysing started");
    rerender(<ProgressPlaceholder {...props(1)} />);
    expect(screen.getByRole("status")).toHaveTextContent("Analysing 1 / 200 games");
    rerender(<ProgressPlaceholder {...props(200)} />);
    expect(screen.getByRole("status")).toHaveTextContent("Analysing finished: 200 games");
  });

  it("throttles count updates to one per 10 seconds, ending on the latest", () => {
    const { rerender } = render(<ProgressPlaceholder {...props(1)} />);
    act(() => {
      vi.advanceTimersByTime(10_000);
    });
    rerender(<ProgressPlaceholder {...props(20)} />);
    expect(screen.getByRole("status")).toHaveTextContent("Analysing 20 / 200 games");
    rerender(<ProgressPlaceholder {...props(21)} />);
    rerender(<ProgressPlaceholder {...props(22)} />);
    act(() => {
      vi.advanceTimersByTime(9_000);
    });
    expect(screen.getByRole("status")).toHaveTextContent("Analysing 20 / 200 games");
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(screen.getByRole("status")).toHaveTextContent("Analysing 22 / 200 games");
  });
});
