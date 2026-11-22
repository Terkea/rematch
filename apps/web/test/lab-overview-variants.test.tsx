// @vitest-environment jsdom
import { fireEvent, render, within } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { OverviewA } from "../src/app/lab/overview/a/view.js";
import { OverviewB } from "../src/app/lab/overview/b/view.js";
import { OverviewC } from "../src/app/lab/overview/c/view.js";
import { toViewB, toViewC } from "../src/app/lab/overview/view.js";
import type { Overview } from "../src/server/overview/types.js";
import { classes, empty, overview } from "./helpers/overview-fixture.js";

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((q: string) => ({
    matches: false,
    media: q,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
});

const renders = {
  a: (o: Overview) => render(<OverviewA overview={o} />),
  // b and c are client components: they get only the narrow view model, never the Overview.
  b: (o: Overview) => render(<OverviewB view={toViewB(o)} />),
  c: (o: Overview) => render(<OverviewC view={toViewC(o, o.accounts.length ? classes() : [])} />),
};

describe.each(["a", "b", "c"] as const)("overview variant %s", (v) => {
  it("never renders NaN, undefined or Infinity", () => {
    const { container } = renders[v](overview());
    expect(container.textContent).not.toMatch(/NaN|undefined|Infinity/);
  });

  it("shows n beside every rate and asks for more games below the minimum", () => {
    const { container } = renders[v](overview());
    const text = container.textContent ?? "";
    expect(text).toMatch(/Based on 33 analysed games|Based on 30 rapid games/);
    expect(text).toContain("Need 7 more games");
    expect(text).toContain("Based on 3 blitz games");
    expect(text).toContain("Every game gave the same value, so there is no interval");
    expect(text).toContain("33 of 35 games analysed.");
  });

  it("shows the connect empty state with no linked account, and no charts", () => {
    const { container, getByText } = renders[v](empty());
    expect(getByText("Connect a chess.com account to see your games here.")).toBeInTheDocument();
    expect(container.querySelector("[data-chart]")).toBeNull();
  });
});

describe("the variants differ on their named axes", () => {
  it("a: one reading column in which each KPI row carries its own chart", () => {
    const { container } = renders.a(overview());
    const layout = container.querySelector('[data-layout="scoresheet"]');
    expect(layout).not.toBeNull();
    const rows = [...container.querySelectorAll("[data-kpi-row]")];
    expect(rows.length).toBeGreaterThanOrEqual(4);
    const accuracy = rows.find((r) => r.getAttribute("data-kpi-row") === "accuracy");
    const blunders = rows.find((r) => r.getAttribute("data-kpi-row") === "blunders");
    expect(accuracy?.querySelector('[data-chart="trend"]')).not.toBeNull();
    expect(blunders?.querySelector('[data-chart="columns"]')).not.toBeNull();
    expect(container.querySelectorAll("[data-region]")).toHaveLength(0);
  });

  it("b: the accuracy trend comes first, then KPI rows beside the judgement columns", () => {
    const { container } = renders.b(overview());
    expect(container.querySelector('[data-layout="chart-led"]')).not.toBeNull();
    const trend = container.querySelector('[data-chart="trend"]');
    const kpis = container.querySelector('[data-region="kpis"]');
    const judgements = container.querySelector('[data-region="judgements"]');
    expect(trend && kpis && judgements).toBeTruthy();
    // The trend precedes the KPI rows, and the KPI rows and columns are siblings in one grid.
    expect(trend?.compareDocumentPosition(kpis as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(kpis?.parentElement).toBe(judgements?.parentElement);
    expect(kpis?.contains(trend as Node)).toBe(false);
    expect(judgements?.querySelector('[data-chart="columns"]')).not.toBeNull();
  });

  it("c: time-control rows are the spine, each expanding to its accuracy and judgements", () => {
    const { container } = renders.c(overview());
    expect(container.querySelector('[data-layout="time-control"]')).not.toBeNull();
    const spine = container.querySelector("[data-spine]");
    const rows = [...(spine?.querySelectorAll(":scope > li") ?? [])];
    expect(rows.map((r) => r.getAttribute("data-time-class"))).toEqual(["rapid", "blitz"]);
    const [rapid, blitz] = rows.map((r) => within(r as HTMLElement).getByRole("button"));
    expect(rapid).toHaveAttribute("aria-expanded", "true");
    expect(blitz).toHaveAttribute("aria-expanded", "false");
    const rapidPanel = document.getElementById(rapid?.getAttribute("aria-controls") ?? "");
    expect(rapidPanel?.querySelector('[data-kpi-row="accuracy"]')).not.toBeNull();
    expect(rapidPanel?.querySelector('[data-kpi-row="blunders"]')).not.toBeNull();
    fireEvent.click(blitz as HTMLElement);
    expect(blitz).toHaveAttribute("aria-expanded", "true");
    const blitzPanel = document.getElementById(blitz?.getAttribute("aria-controls") ?? "");
    expect(blitzPanel).toBeVisible();
    expect(blitzPanel?.textContent).toContain("Need 7 more games");
  });
});
