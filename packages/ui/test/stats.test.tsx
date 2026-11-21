import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { kpiFixtures } from "../src/fixtures/index.js";
import { KpiStat, Sparkline, Stat } from "../src/stats/index.js";

afterEach(cleanup);

/** The sparkline's line path (the one with a `d` that draws the series). */
const linePath = (container: HTMLElement) =>
  container.querySelector<SVGPathElement>("path[data-part='line']");
const moves = (d: string) => (d.match(/M/g) ?? []).length;
const vertices = (d: string) => (d.match(/[ML]/g) ?? []).length;

describe("Sparkline", () => {
  it("draws one path through every point", () => {
    const { container } = render(<Sparkline values={[1, 2, 3]} label="Accuracy trend" />);
    const d = linePath(container)?.getAttribute("d") ?? "";
    expect(container.querySelectorAll("path[data-part='line']")).toHaveLength(1);
    expect(moves(d)).toBe(1);
    expect(vertices(d)).toBe(3);
  });

  it("breaks the line at a missing value", () => {
    const { container } = render(<Sparkline values={[1, null, 3]} label="Accuracy trend" />);
    const d = linePath(container)?.getAttribute("d") ?? "";
    expect(moves(d)).toBe(2);
  });

  it("marks the last point with an ink dot", () => {
    const { container } = render(<Sparkline values={[1, 2, 3]} label="Accuracy trend" />);
    const dot = container.querySelector("[data-part='last']");
    expect(dot).not.toBeNull();
    expect(dot?.getAttribute("class") ?? "").toContain("stroke-ink");
  });

  it("has no axes and names itself", () => {
    const { container } = render(<Sparkline values={[1, 2, 3]} label="Accuracy trend" />);
    expect(screen.getByRole("img", { name: "Accuracy trend" })).toBeInTheDocument();
    expect(container.querySelector("line, text")).toBeNull();
  });

  it.each([[[]], [[4]], [[null, 4, null]]])("says there are not enough games for %j", (values) => {
    const { container } = render(<Sparkline values={values} label="Accuracy trend" />);
    expect(screen.getByLabelText("Not enough games yet")).toBeInTheDocument();
    expect(container.querySelector("path")).toBeNull();
  });
});

/** NumberFlow's light-DOM text: the formatted value it renders before its shadow root takes over. */
const flowText = (container: HTMLElement) =>
  container.querySelector("number-flow-react > span")?.textContent;

describe("Stat", () => {
  it("renders a percent with one decimal", () => {
    const { container } = render(<Stat value={0.642} format="percent" locale="en-US" />);
    expect(flowText(container)).toBe("64.2%");
  });

  it("renders a decimal with two places and an integer with none", () => {
    const { container: a } = render(<Stat value={1.8} format="decimal" locale="en-US" />);
    expect(flowText(a)).toBe("1.80");
    const { container: b } = render(<Stat value={38} format="integer" locale="en-US" />);
    expect(flowText(b)).toBe("38");
  });
});

const base = {
  label: "Rating-adjusted score",
  value: 64.2,
  format: "decimal" as const,
  interval: { low: 61, high: 67.5, level: 0.8 as const },
  basis: { games: 38, timeClass: "rapid" },
};

describe("KpiStat", () => {
  it("states its basis and describes the band in words", () => {
    render(<KpiStat {...base} />);
    expect(screen.getByText("Based on 38 rapid games")).toBeInTheDocument();
    expect(screen.getByText("80% likely between 61.0 and 67.5")).toBeInTheDocument();
  });

  it("formats a percent band the way the value is formatted", () => {
    const accuracy = kpiFixtures.items[0];
    if (!accuracy) throw new Error("fixture missing");
    render(<KpiStat {...accuracy} />);
    expect(screen.getByText("80% likely between 71.8% and 76.6%")).toBeInTheDocument();
  });

  it("shows an early estimate only when provisional", () => {
    const { rerender } = render(<KpiStat {...base} />);
    expect(screen.queryByText("Early estimate")).toBeNull();
    rerender(<KpiStat {...base} provisional />);
    expect(screen.getByText("Early estimate")).toBeInTheDocument();
  });

  it("draws the band, the value tick and the target tick", () => {
    const { container } = render(
      <KpiStat {...base} target={{ value: 70, label: "Next band" }} trend={[60, 62, 64.2]} />,
    );
    expect(container.querySelector("[data-part='band']")?.getAttribute("class")).toContain(
      "fill-rule-strong",
    );
    expect(container.querySelector("[data-part='value']")?.getAttribute("class")).toContain(
      "stroke-ink",
    );
    expect(container.querySelector("[data-part='target']")?.getAttribute("class")).toContain(
      "stroke-good",
    );
    // The target is named in text too, so it never rests on colour alone.
    expect(screen.getByText("Next band 70.0")).toBeInTheDocument();
    expect(container.querySelector("path[data-part='line']")).not.toBeNull();
  });

  it("places the value tick inside the band on the scale", () => {
    const { container } = render(<KpiStat {...base} />);
    const band = container.querySelector("[data-part='band']");
    const tick = container.querySelector("[data-part='value']");
    const x = Number.parseFloat(tick?.getAttribute("x1") ?? "");
    const left = Number.parseFloat(band?.getAttribute("x") ?? "");
    const right = left + Number.parseFloat(band?.getAttribute("width") ?? "");
    expect(x).toBeGreaterThan(left);
    expect(x).toBeLessThan(right);
  });
});
