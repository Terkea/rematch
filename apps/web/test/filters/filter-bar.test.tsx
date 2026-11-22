// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CompareBlock } from "../../src/app/(app)/app/_filters/compare-block.js";
import { compareView } from "../../src/app/(app)/app/_filters/compare-view.js";
import { FilterBar } from "../../src/app/(app)/app/_filters/filter-bar.js";
import {
  type BarConfig,
  filterBarModel,
  parseBarFilters,
} from "../../src/app/(app)/app/_filters/model.js";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { comparePeriodRows } from "../../src/server/stats/period-compare.js";

vi.mock("next/link", () => ({
  default: ({
    href,
    children,
    scroll: _s,
    ...rest
  }: {
    href: string;
    children: React.ReactNode;
    scroll?: boolean;
  }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

const cfg: BarConfig = {
  basePath: "/app",
  timeClasses: [
    { tc: "rapid", games: 120 },
    { tc: "blitz", games: 40 },
  ],
  allTimeClasses: true,
  formats: [{ key: "600+0", label: "10+0", games: 30 }],
  color: "on",
  dates: true,
  compare: true,
  now: new Date("2026-11-22T12:00:00Z"),
};
const bar = (q: string, c: Partial<BarConfig> = {}) => {
  const conf = { ...cfg, ...c };
  return <FilterBar model={filterBarModel(parseBarFilters(new URLSearchParams(q), conf), conf)} />;
};

describe("FilterBar", () => {
  it("is links and GET forms, so it works without JavaScript", () => {
    const { container } = render(bar("color=black"));
    const desktop = container.querySelector("section > div") as HTMLElement;
    const colour = within(desktop).getByRole("group", { name: "Colour" });
    expect(within(colour).getByRole("link", { name: "Black" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(within(colour).getByRole("link", { name: "White" })).toHaveAttribute(
      "href",
      "/app?color=white",
    );
    const form = desktop.querySelector("form[method='get']") as HTMLFormElement;
    expect(form.getAttribute("action")).toBe("/app");
    expect(form.querySelector("input[type='date'][name='from']")).not.toBeNull();
    expect(form.querySelector("input[type='hidden'][name='color']")).toHaveAttribute(
      "value",
      "black",
    );
  });

  it("folds into one Filters disclosure on phones, naming the active filters", () => {
    const { container } = render(bar("color=black&cmp=month"));
    const summary = container.querySelector("section > details > summary") as HTMLElement;
    expect(summary).toHaveTextContent("Filters (2)");
    expect(summary).toHaveTextContent("Rapid, Black, this month vs last");
  });

  it("says honestly where the stored data has no colour split", () => {
    render(bar("", { color: "coming" }));
    expect(screen.getAllByText(/Colour split for weaknesses is coming/)[0]).toBeInTheDocument();
    expect(screen.queryAllByRole("link", { name: "White" })).toHaveLength(0);
  });
});

function rows(n: number, acc: number): GameStatRow[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `g${i}`,
    playedAt: new Date(Date.UTC(2026, 9, 1 + (i % 28))),
    color: "white",
    result: "win",
    analysed: true,
    accuracy: acc + (i % 3),
    blunders: 1,
    mistakes: 1,
    inaccuracies: 1,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
  }));
}

describe("CompareBlock", () => {
  it("shows A, B and the verdict per stat, with n on each side", () => {
    const view = compareView({
      periods: {
        a: { kind: "days", from: "2026-09-01", to: "2026-09-30" },
        b: { kind: "days", from: "2026-10-01", to: "2026-10-31" },
      },
      spans: { a: null, b: null },
      comparison: comparePeriodRows(rows(20, 60), rows(4, 60), "t"),
    });
    render(<CompareBlock view={view} />);
    const acc = screen.getByRole("row", { name: /Accuracy/ });
    expect(acc).toHaveTextContent("20 analysed games");
    expect(acc).toHaveTextContent("Need 6 more games, 4 analysed games");
    expect(acc).toHaveTextContent("Not enough games to tell: need 6 more in period B");
    expect(document.body.textContent).not.toMatch(/NaN|Infinity|undefined/);
  });
});
