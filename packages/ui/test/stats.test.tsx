import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { kpiFixtures } from "../src/fixtures/index.js";
import { bandEnds, domainOf, KpiStat, Sparkline, Stat } from "../src/stats/index.js";
import { tokensCss } from "./design-md.js";

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

  it("reveals the subpaths after a gap in order, through one clip", () => {
    const { container } = render(
      <Sparkline values={[1, 2, null, 3, 4]} label="Accuracy trend" drawIn />,
    );
    const line = linePath(container);
    const clip = line?.closest("[clip-path]");
    expect(clip).not.toBeNull();
    // No dash reveal: dashes restart per subpath, so the second piece would draw alongside the first.
    expect(line?.style.strokeDasharray).toBe("");
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
    expect(
      screen.getByRole("img", { name: "Accuracy trend: not enough games yet" }),
    ).toBeInTheDocument();
    expect(container.querySelector("path")).toBeNull();
  });
});

describe("Sparkline scale", () => {
  it("spans at least minSpan, centred on the series range", () => {
    const { container } = render(
      <Sparkline values={[0.69, 0.7, 0.71]} label="Accuracy trend" minSpan={0.1} />,
    );
    const svg = container.querySelector("svg");
    expect(Number(svg?.getAttribute("data-min"))).toBeCloseTo(0.65);
    expect(Number(svg?.getAttribute("data-max"))).toBeCloseTo(0.75);
  });

  it("keeps the series range when it is wider than minSpan", () => {
    const { container } = render(<Sparkline values={[1, 5]} label="Blunders trend" minSpan={2} />);
    const svg = container.querySelector("svg");
    expect(Number(svg?.getAttribute("data-min"))).toBe(1);
    expect(Number(svg?.getAttribute("data-max"))).toBe(5);
  });

  it("KpiStat scales the accuracy trend to twice its 80% interval", () => {
    const accuracy = kpiFixtures.items[0];
    if (!accuracy) throw new Error("fixture missing");
    const { container } = render(<KpiStat {...accuracy} />);
    const svg = container.querySelector("svg[data-min]");
    const min = Number(svg?.getAttribute("data-min"));
    const max = Number(svg?.getAttribute("data-max"));
    // Interval 71.8%–76.6% is 4.8 points, so the trend spans 9.6 points around 69%–74.2%.
    expect(max - min).toBeCloseTo(2 * (0.766 - 0.718));
    expect((max + min) / 2).toBeCloseTo((0.69 + 0.742) / 2);
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
      <KpiStat {...base} target={{ value: 70, label: "Next target" }} trend={[60, 62, 64.2]} />,
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
    expect(screen.getByText("Next target 70.0")).toBeInTheDocument();
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

  it("takes its heading level from a prop", () => {
    render(<KpiStat {...base} headingLevel={4} />);
    expect(screen.getByRole("heading", { level: 4, name: base.label })).toBeInTheDocument();
  });
});

const pctKpi = (low: number, high: number, value: number, target?: number) => ({
  label: "Accuracy",
  value,
  format: "percent" as const,
  interval: { low, high, level: 0.8 as const },
  basis: { games: 11, timeClass: "rapid" },
  ...(target === undefined ? {} : { target: { value: target, label: "Next target" } }),
});

describe("domainOf", () => {
  it("places a 40-point window around the band, value and target", () => {
    const [lo, hi] = domainOf(pctKpi(0.718, 0.766, 0.742, 0.78));
    expect(hi - lo).toBeCloseTo(0.4);
    expect(lo).toBeLessThanOrEqual(0.718);
    expect(hi).toBeGreaterThanOrEqual(0.78);
  });

  it("clamps the window at 0% and at 100%", () => {
    expect(domainOf(pctKpi(0.02, 0.09, 0.05))).toEqual([0, 0.4]);
    const [lo, hi] = domainOf(pctKpi(0.93, 0.99, 0.96));
    expect(lo).toBeCloseTo(0.6);
    expect(hi).toBeCloseTo(1);
  });

  it("never falls back to 0–100%: a 46-point interval keeps the window around the value", () => {
    const [lo, hi] = domainOf(pctKpi(0.3, 0.76, 0.53));
    expect(hi - lo).toBeCloseTo(0.4);
    expect(lo).toBeLessThanOrEqual(0.53);
    expect(hi).toBeGreaterThanOrEqual(0.53);
  });

  it("includes a negative target in a decimal domain", () => {
    const [lo, hi] = domainOf({
      ...base,
      value: -0.5,
      interval: { low: -0.8, high: -0.2, level: 0.8 },
      target: { value: -1, label: "Next target" },
    });
    expect(lo).toBeLessThanOrEqual(-1);
    expect(hi).toBeGreaterThanOrEqual(0);
  });
});

describe("KpiStat honest band", () => {
  it("draws a 46-point provisional band wider than a 25-point one, with an end cap", () => {
    const wide = render(<KpiStat {...pctKpi(0.3, 0.76, 0.53)} provisional />);
    const wideBand = wide.container.querySelector("[data-part='band']");
    const wideW = Number.parseFloat(wideBand?.getAttribute("width") ?? "");
    expect(wide.container.querySelector("[data-part^='band-continues']")).not.toBeNull();
    expect(wideBand?.getAttribute("class") ?? "").not.toContain("opacity");
    // The text stays exact.
    expect(screen.getByText("80% likely between 30.0% and 76.0%")).toBeInTheDocument();
    cleanup();
    const narrow = render(<KpiStat {...pctKpi(0.48, 0.73, 0.612)} />);
    const narrowW = Number.parseFloat(
      narrow.container.querySelector("[data-part='band']")?.getAttribute("width") ?? "",
    );
    expect(wideW).toBeGreaterThan(narrowW);
    expect(narrow.container.querySelector("[data-part^='band-continues']")).toBeNull();
  });

  it("rounds decimal ends outward, never inside the interval or past the value", () => {
    const cases: [number, number, number][] = [
      [1.52, 2.16, 1.84],
      [61.04, 67.46, 64.2],
      [0.149, 0.151, 0.15],
      [2.95, 3.05, 3.0],
      [1.2, 1.3, 1.25],
    ];
    for (const [low, high, value] of cases) {
      const [l, h] = bandEnds({ low, high, level: 0.8 }, "decimal");
      const rounded = Math.round(value * 100) / 100;
      expect(l).toBeLessThanOrEqual(low + 1e-9);
      expect(h).toBeGreaterThanOrEqual(high - 1e-9);
      expect(l).toBeLessThanOrEqual(rounded);
      expect(h).toBeGreaterThanOrEqual(rounded);
    }
    expect(bandEnds({ low: 1.52, high: 2.16, level: 0.8 }, "decimal")).toEqual([1.5, 2.2]);
    expect(bandEnds({ low: 0.718, high: 0.766, level: 0.8 }, "percent")).toEqual([0.718, 0.766]);
  });

  it("gives the target one more place when it would read the same as an interval end", () => {
    render(
      <KpiStat
        {...base}
        value={1.84}
        interval={{ low: 1.52, high: 2.16, level: 0.8 }}
        target={{ value: 1.5, label: "Next target" }}
      />,
    );
    expect(screen.getByText("80% likely between 1.5 and 2.2")).toBeInTheDocument();
    expect(screen.getByText("Next target 1.50")).toBeInTheDocument();
  });

  it("reads --stat-surface from its host and never sets it", () => {
    const { container } = render(
      <div style={{ "--stat-surface": "rgb(1 2 3)" } as never}>
        <KpiStat {...base} trend={[60, 62, 64.2]} />
      </div>,
    );
    const host = container.firstElementChild as HTMLElement;
    const setters = [...host.querySelectorAll("*")].filter(
      (el) =>
        (el.getAttribute("class") ?? "").includes("[--stat-surface:") ||
        (el.getAttribute("style") ?? "").includes("--stat-surface:"),
    );
    expect(setters).toEqual([]);
    const halos = [...host.querySelectorAll("[class*='stat-surface']")];
    expect(halos.length).toBeGreaterThan(0);
    for (const el of halos) {
      expect(el.getAttribute("class")).toContain("var(--stat-surface,var(--color-sheet))");
    }
    expect(tokensCss).not.toMatch(/--stat-surface\s*:/);
  });
});

describe("Stat accessible text", () => {
  it.each([
    ["percent", 0.642, "64.2%"],
    ["decimal", 1.84, "1.84"],
    ["integer", 1234, "1,234"],
  ] as const)(
    "writes the %s value as text a reader and find-in-page can use",
    (format, value, text) => {
      const { container } = render(<Stat value={value} format={format} />);
      expect(container.querySelector("[data-part=stat-text]")).toHaveTextContent(text);
      expect(container).toHaveTextContent(text);
      expect(container.querySelector("number-flow-react")?.getAttribute("aria-hidden")).toBe(
        "true",
      );
    },
  );
});
