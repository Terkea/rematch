import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  AreaScore,
  AreaScoreKey,
  AreaScoreList,
  areaLabels,
  rulerDomain,
} from "../src/area-score/index.js";
import { areaScoreFixtures } from "../src/fixtures/index.js";

afterEach(cleanup);

describe("AreaScoreList", () => {
  it("renders the seven area labels in order, as a list", () => {
    render(<AreaScoreList items={areaScoreFixtures.items} />);
    const list = screen.getByRole("list");
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(7);
    const labels = items.map((li) => li.querySelector("[data-area-label]")?.textContent);
    expect(labels).toEqual([
      "Openings",
      "Middlegame",
      "Endgame",
      "Tactics",
      "Calculation",
      "Time",
      "Mindset",
    ]);
    expect(Object.values(areaLabels)).toEqual(labels);
  });

  it("gives the endgame row its accessible text", () => {
    render(<AreaScoreList items={areaScoreFixtures.items} />);
    const endgame = screen.getAllByRole("listitem")[2];
    if (!endgame) throw new Error("no endgame row");
    expect(
      within(endgame).getByText("Endgame 62, band median 55, next band 68"),
    ).toBeInTheDocument();
  });

  it("places the score, median and next-band ticks on a data-driven ruler", () => {
    // 62 / 55 / 68: 50 to 80 after the margin, widened to the 40-point minimum: 40 to 80.
    const { container } = render(
      <AreaScore
        area="endgame"
        score={62}
        band={{ median: 55, next: 68 }}
        basis={{ games: 22, timeClass: "rapid" }}
      />,
    );
    const left = (name: string) =>
      (container.querySelector(`[data-tick="${name}"]`) as HTMLElement | null)?.style.left;
    expect(left("median")).toBe("37.5%");
    expect(left("next")).toBe("70%");
    // The needle moves by transform on the one clock, never by `left`.
    const needle = container.querySelector("[data-needle]") as HTMLElement;
    expect(needle.style.transform).toBe("translateX(calc(var(--sweep, 1) * 55%))");
    // A standalone row numbers its own scale.
    expect(container.querySelector("[data-scale]")?.textContent).toBe("406080");
  });

  it("shares one domain across the list, numbered once", () => {
    const { container } = render(<AreaScoreList items={areaScoreFixtures.items} />);
    expect(container.querySelectorAll("[data-scale]")).toHaveLength(1);
    expect(container.querySelector("[data-scale]")?.textContent).toBe("406080");
    // Openings 71 on 40 to 80.
    const first = container.querySelector("[data-needle]") as HTMLElement;
    expect(first.style.transform).toBe("translateX(calc(var(--sweep, 1) * 77.5%))");
  });

  it("derives the domain from the data, floored and ceiled to 10, at least 40 wide", () => {
    const row = (score: number, median: number, next: number) => ({
      score,
      band: { median, next },
    });
    expect(rulerDomain(areaScoreFixtures.items)).toEqual([40, 80]);
    expect(rulerDomain([row(20, 35, 90)])).toEqual([10, 100]);
    expect(rulerDomain([row(96, 97, 99)])).toEqual([60, 100]);
    expect(rulerDomain([row(2, 3, 5)])).toEqual([0, 40]);
  });
});

describe("AreaScore delta", () => {
  const base = {
    area: "endgame" as const,
    score: 62,
    band: { median: 55, next: 68 },
    basis: { games: 22, timeClass: "rapid" },
  };

  it("renders a delta of +4 as text", () => {
    render(<AreaScore {...base} delta={4} />);
    expect(screen.getByText("+4 since last report")).toBeInTheDocument();
  });

  it("uses the real minus sign for a drop, and says so when nothing changed", () => {
    const { unmount } = render(<AreaScore {...base} delta={-5} />);
    expect(screen.getByText("−5 since last report")).toBeInTheDocument();
    unmount();
    render(<AreaScore {...base} delta={0} />);
    expect(screen.getByText("No change since last report")).toBeInTheDocument();
  });

  it("names its basis", () => {
    render(<AreaScore {...base} />);
    expect(screen.getByText("22 rapid games")).toBeInTheDocument();
  });
});

describe("AreaScore without a measured next band", () => {
  const base = {
    area: "endgame" as const,
    score: 62,
    band: { median: 50 },
    basis: { games: 22, timeClass: "rapid" },
  };

  it("area score without next band draws no next tick", () => {
    const { container } = render(<AreaScore {...base} />);
    expect(container.querySelector('[data-tick="next"]')).toBeNull();
    expect(container.querySelector('[data-tick="median"]')).not.toBeNull();
    expect(
      screen.getByText("Endgame 62, band median 50, next band not measured yet"),
    ).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/NaN|undefined|Infinity/);
  });

  it("fits the domain without a next band", () => {
    // 50 to 62: 40 to 70 after the margin, widened at the low end first to 30 to 70.
    expect(rulerDomain([{ score: 62, band: { median: 50 } }])).toEqual([30, 70]);
  });

  it("drops the next-band entry from the key when asked", () => {
    render(<AreaScoreKey next={false} />);
    expect(screen.queryByText("Next band")).toBeNull();
    expect(screen.getByText("Band median")).toBeInTheDocument();
  });
});

describe("AreaScore interval", () => {
  const base = {
    area: "endgame" as const,
    score: 62,
    band: { median: 50 },
    basis: { games: 22, timeClass: "rapid" },
  };

  it("draws a stored interval and states it", () => {
    const { container } = render(
      <AreaScore {...base} interval={{ low: 55, high: 68, level: 0.8 }} />,
    );
    expect(container.querySelector('[data-part="interval"]')).not.toBeNull();
    expect(container.textContent).toContain("80% range 55–68");
  });

  it("says no interval yet when there is none", () => {
    const { container } = render(<AreaScore {...base} interval={null} />);
    expect(container.querySelector('[data-part="interval"]')).toBeNull();
    expect(container.textContent).toContain("No interval yet");
  });

  it("explains the connector and the interval in the key", () => {
    render(<AreaScoreKey next={false} interval />);
    expect(screen.getByText("Distance to the band median")).toBeInTheDocument();
    expect(screen.getByText("Likely range")).toBeInTheDocument();
  });
});
