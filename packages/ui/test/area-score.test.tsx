import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AreaScore, AreaScoreList, areaLabels } from "../src/area-score/index.js";
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

  it("places the score, median and next-band ticks on a 0-100 ruler", () => {
    const { container } = render(
      <AreaScore
        area="endgame"
        score={62}
        band={{ median: 55, next: 68 }}
        basis={{ games: 22, timeClass: "rapid" }}
      />,
    );
    const at = (name: string) =>
      (container.querySelector(`[data-tick="${name}"]`) as HTMLElement | null)?.style.left;
    expect(at("median")).toBe("55%");
    expect(at("next")).toBe("68%");
    expect(container.querySelector('[data-tick="score"]')).not.toBeNull();
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
