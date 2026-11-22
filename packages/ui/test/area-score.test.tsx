import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AreaScore, AreaScoreList, areaLabels, rulerDomain } from "../src/area-score/index.js";
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

// Web polish critic I6: a "next band" flag below the player's score contradicts its own legend.
describe("AreaScore past the next band", () => {
  const past = {
    area: "time" as const,
    score: 74,
    band: { median: 62, next: 71 },
    basis: { games: 38, timeClass: "rapid" },
  };

  it("draws no next-band flag once the score has reached it, and says so", () => {
    const { container } = render(<AreaScore {...past} />);
    expect(container.querySelector('[data-tick="next"]')).toBeNull();
    expect(screen.getByText("In the next band (71 and up)")).toBeInTheDocument();
    expect(
      screen.getByText("Time 74, band median 62, in the next band (71 and up)"),
    ).toBeInTheDocument();
  });

  it("keeps the flag ahead of a score below it", () => {
    const { container } = render(<AreaScore {...past} score={70} />);
    expect(container.querySelector('[data-tick="next"]')).not.toBeNull();
    expect(screen.queryByText(/In the next band/u)).toBeNull();
  });

  it("never draws a flag left of the needle in the fixtures", () => {
    const { container } = render(<AreaScoreList items={areaScoreFixtures.items} />);
    const rows = container.querySelectorAll("li");
    areaScoreFixtures.items.forEach((item, i) => {
      const flag = rows[i]?.querySelector('[data-tick="next"]');
      expect(flag === null || item.band.next > item.score).toBe(true);
    });
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
