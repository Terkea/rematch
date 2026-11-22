import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ProgressPlaceholder } from "../src/progress/index.js";

afterEach(cleanup);

describe("ProgressPlaceholder", () => {
  it("states done / total and the unit when the total is known", () => {
    render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 38, total: 200, unit: "rapid games" }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Analysing 38 / 200 rapid games");
  });

  it("says 'so far' when the total is unknown", () => {
    render(
      <ProgressPlaceholder shape="list" verb="Importing" progress={{ done: 120, unit: "games" }} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Importing: 120 games so far");
  });

  it("shows the verb alone without progress", () => {
    render(<ProgressPlaceholder shape="stat" verb="Analysing" />);
    expect(screen.getByRole("status").textContent?.trim()).toBe("Analysing");
  });

  it("is a polite live status region", () => {
    render(<ProgressPlaceholder shape="board" verb="Analysing" />);
    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
  });

  it("never uses a spinner", () => {
    const { container } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 1, total: 2, unit: "games" }}
      />,
    );
    expect(container.querySelector(".animate-spin")).toBeNull();
  });

  it("grows the pencil stroke with done / total, only when the total is known", () => {
    const { container, rerender } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 50, total: 200, unit: "games" }}
      />,
    );
    const stroke = container.querySelector("[data-part='stroke']");
    expect(stroke).toHaveAttribute("data-fraction", "0.25");
    rerender(
      <ProgressPlaceholder shape="graph" verb="Importing" progress={{ done: 5, unit: "games" }} />,
    );
    expect(container.querySelector("[data-part='stroke']")).toBeNull();
  });

  it("clamps the fraction to 1 and handles a zero total", () => {
    const { container, rerender } = render(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 9, total: 4, unit: "games" }}
      />,
    );
    expect(container.querySelector("[data-part='stroke']")).toHaveAttribute("data-fraction", "1");
    rerender(
      <ProgressPlaceholder
        shape="graph"
        verb="Analysing"
        progress={{ done: 0, total: 0, unit: "games" }}
      />,
    );
    expect(container.querySelector("[data-part='stroke']")).toHaveAttribute("data-fraction", "0");
  });

  it("renders the basis note and a skeleton per shape", () => {
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
      expect(container.querySelector(`[data-shape='${shape}']`)).not.toBeNull();
    }
  });
});
