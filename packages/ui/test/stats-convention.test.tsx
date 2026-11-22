import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { kpiFixtures } from "../src/fixtures/index.js";
import { INTERVAL_LEVEL, KPI_MIN_GAMES, KpiStat } from "../src/stats/index.js";

afterEach(cleanup);

// One convention for every KPI (ADR 0047 R3, ADR-R2-5 "80% everywhere", web polish critic I7):
// no headline number below 10 games, and every interval is an 80% one.
const kpi = (games: number) => ({
  label: "Score in time scrambles",
  value: 0.53,
  format: "percent" as const,
  interval: { low: 0.3, high: 0.76, level: 0.8 as const },
  basis: { games, timeClass: "rapid" },
  trend: [0.45, null, 0.53],
});

describe("the KPI convention", () => {
  it("is 10 games and an 80% interval", () => {
    expect(KPI_MIN_GAMES).toBe(10);
    expect(INTERVAL_LEVEL).toBe(0.8);
  });

  it("shows no headline, band or trend below the minimum: a dash and the shortfall", () => {
    const { container } = render(<KpiStat {...kpi(6)} provisional />);
    expect(screen.getByText("Based on 6 rapid games")).toBeInTheDocument();
    expect(screen.getByText("Need 4 more games")).toBeInTheDocument();
    expect(container.querySelector("number-flow-react")).toBeNull();
    expect(container.querySelector("[data-part='band']")).toBeNull();
    expect(container.querySelector("path[data-part='line']")).toBeNull();
    expect(container.textContent).not.toContain("likely between");
    expect(container.textContent).not.toContain("53");
  });

  it("names one missing game in the singular", () => {
    render(<KpiStat {...kpi(9)} />);
    expect(screen.getByText("Need 1 more game")).toBeInTheDocument();
  });

  it("shows the estimate from the minimum on", () => {
    const { container } = render(<KpiStat {...kpi(10)} />);
    expect(container.querySelector("number-flow-react")).not.toBeNull();
    expect(screen.getByText("80% likely between 30.0% and 76.0%")).toBeInTheDocument();
  });

  it("every fixture KPI uses the one interval level", () => {
    for (const k of kpiFixtures.items) expect(k.interval.level).toBe(INTERVAL_LEVEL);
  });

  it("every fixture KPI shown as a number has at least the minimum games", () => {
    render(
      <div>
        {kpiFixtures.items.map((k) => (
          <KpiStat key={k.label} {...k} />
        ))}
      </div>,
    );
    const shown = kpiFixtures.items.filter((k) => k.basis.games >= KPI_MIN_GAMES);
    expect(screen.getAllByText(/^80% likely between/u)).toHaveLength(shown.length);
    expect(screen.queryAllByText(/^(?!80)\d+% likely/u)).toHaveLength(0);
  });
});
