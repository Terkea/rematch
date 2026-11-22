import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  ExemplarBoard,
  exemplarCaption,
  formatKpiValue,
  KpiTrendGrid,
  type KpiTrendItem,
  kpiSummary,
  kpiYScale,
  WeaknessRow,
  type WeaknessRowProps,
  weaknessRateText,
} from "../src/profile/index.js";

afterEach(cleanup);

const BAD = /NaN|undefined|Infinity|null/;

const row: WeaknessRowProps = {
  label: "Hanging pieces",
  state: "active",
  rate: { mean: 0.0421, low: 0.031, high: 0.056 },
  evidence: { events: 38, chances: 902, sufficient: true, needMore: null },
  band: { median: 0.021 },
  href: "/app/weaknesses/W1",
  trend: [0.05, 0.047, null, 0.043, 0.041],
  onset: "2026-10-14",
};

describe("weaknessRateText", () => {
  it("formats mean, range and counts", () => {
    expect(
      weaknessRateText({ mean: 0.0421, low: 0.031, high: 0.056 }, { events: 38, chances: 902 }),
    ).toBe("Estimated 4.2% of chances (80% range 3.1–5.6%), from 38 of 902");
  });
});

describe("WeaknessRow", () => {
  it("links to the detail and shows the rate with its counts", () => {
    const { container } = render(<WeaknessRow {...row} />);
    expect(screen.getByRole("link", { name: /Hanging pieces/ })).toHaveAttribute(
      "href",
      "/app/weaknesses/W1",
    );
    expect(container.textContent).toContain("38 of 902 chances");
    expect(container.textContent).toContain("Active");
    // The interval separates from the median: the comparison says so, and names the median.
    expect(container.textContent).toContain("Band median 2.1%");
    expect(container.querySelector("time")).toHaveAttribute("dateTime", "2026-10-14");
    expect(container.textContent).not.toMatch(BAD);
  });

  it("insufficient row says early read", () => {
    const { container } = render(
      <WeaknessRow
        {...row}
        state="emerging"
        evidence={{ events: 2, chances: 40, sufficient: false, needMore: 180 }}
      />,
    );
    expect(container.textContent).toContain("Early read");
    expect(container.textContent).toContain("180 more chances needed");
    expect(container.textContent).not.toMatch(/band median/i);
    expect(container.textContent).not.toMatch(BAD);
  });

  it("says when there is no band reference, never a zero", () => {
    const { container } = render(<WeaknessRow {...row} band={{ median: null }} onset={null} />);
    expect(container.textContent).toContain("No band reference yet");
    expect(container.querySelector("time")).toBeNull();
    expect(container.textContent).not.toMatch(BAD);
  });
});

const weeks = ["2026-11-02", "2026-11-09", "2026-11-16"];
const item = (
  values: (number | null)[],
  extra: Partial<KpiTrendItem["points"][number]>[] = [],
): KpiTrendItem => ({
  metric: "blunders_per_game",
  label: "Blunders per game",
  unit: "per_game",
  points: values.map((value, i) => ({
    week: weeks[i] ?? "2026-11-23",
    value,
    low: value === null ? null : value - 0.3,
    high: value === null ? null : value + 0.3,
    n: value === null ? 0 : 12,
    ...extra[i],
  })),
});
const range = { from: "2026-11-02", to: "2026-11-16" };

describe("KpiTrendGrid", () => {
  it("null points are gaps, not zeros", () => {
    const it0 = item([1.2, null, 0.8]);
    const { container } = render(<KpiTrendGrid items={[it0]} range={range} minN={5} />);
    expect(container.querySelectorAll("[data-part='segment']")).toHaveLength(2);
    const summary = kpiSummary(it0, 5);
    expect(summary).toBe(
      "Blunders per game: 1.20 in week of 2 Nov 2026, 12 games; no games in week of 9 Nov 2026; 0.80 in week of 16 Nov 2026, 12 games.",
    );
    expect(container.textContent).toContain("no games in week of 9 Nov 2026");
    expect(container.textContent).not.toMatch(BAD);
  });

  it("points below minN are hollow", () => {
    const it0 = item([1.2, 1.1, 0.8], [{}, { n: 3 }, {}]);
    const { container } = render(<KpiTrendGrid items={[it0]} range={range} minN={5} />);
    expect(container.querySelectorAll("[data-part='hollow']")).toHaveLength(1);
    // The thin week breaks the line on both sides of it.
    expect(container.querySelectorAll("[data-part='segment']")).toHaveLength(2);
    expect(kpiSummary(it0, 5)).toContain(
      "1.10 in week of 9 Nov 2026, 3 games, below the 5-game minimum",
    );
  });

  it("stored band is drawn as given", () => {
    const it0 = item([1.2, 1.0, 0.8]);
    const { container } = render(<KpiTrendGrid items={[it0]} range={range} minN={5} />);
    const band = container.querySelector("[data-part='band']");
    const d = band?.getAttribute("d") ?? "";
    const y = kpiYScale(it0);
    const ys = [...d.matchAll(/[ML]\s*([\d.-]+)[ ,]([\d.-]+)/g)].map((m) => Number(m[2]));
    // Upper edge then lower edge: the first point is high[0], the last is low[0].
    expect(ys[0]).toBeCloseTo(y(1.5), 6);
    expect(ys[ys.length - 1]).toBeCloseTo(y(0.9), 6);
    expect(ys).toContainEqual(expect.closeTo(y(1.1), 6)); // high[2]
    expect(ys).toContainEqual(expect.closeTo(y(0.5), 6)); // low[2]
  });

  it("an item with no values says so instead of drawing", () => {
    const { container } = render(
      <KpiTrendGrid items={[item([null, null, null])]} range={range} minN={5} />,
    );
    expect(container.textContent).toContain("No games in this range yet");
    expect(container.querySelector("[data-part='segment']")).toBeNull();
    expect(container.textContent).not.toMatch(BAD);
  });

  it("draws report dates as events on the shared axis", () => {
    const { container } = render(
      <KpiTrendGrid
        items={[
          item([1.2, 1.0, 0.8]),
          { ...item([0.7, 0.72, 0.75]), metric: "acc", label: "Accuracy", unit: "pct" },
        ]}
        range={range}
        minN={5}
        events={[{ date: "2026-11-09", label: "Report" }]}
      />,
    );
    expect(container.querySelectorAll("[data-part='event']")).toHaveLength(2);
    expect(container.textContent).toContain("9 Nov 2026: Report");
  });
});

describe("formatKpiValue", () => {
  it("formats each unit", () => {
    expect(formatKpiValue(0.0421, "rate")).toBe("4.2%");
    expect(formatKpiValue(74.25, "pct")).toBe("74.3%");
    expect(formatKpiValue(1.4, "per_game")).toBe("1.40");
    expect(formatKpiValue(12.4, "seconds")).toBe("12 s");
    expect(formatKpiValue(1512.2, "elo")).toBe("1,512");
    expect(formatKpiValue(Number.NaN, "pct")).toBe("not measured");
  });
});

describe("ExemplarBoard", () => {
  // 1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4: the trap. 4. Nxe5? walks into 4… Qg5; 4. Nxd4 was better.
  const fen = "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
  const props = {
    fen,
    played: { san: "Nxe5", uci: "f3e5" },
    best: { san: "Nxd4", uci: "f3d4" },
    refutation: "d8g5",
    caption: { judgement: "mistake" as const, winBefore: 52.4, winAfter: 36.1 },
  };

  it("draws played, best and refutation arrows", () => {
    const { container } = render(<ExemplarBoard {...props} />);
    const tones = new Set(
      [...container.querySelectorAll("[data-mark='arrow']")].map((a) =>
        a.getAttribute("data-tone"),
      ),
    );
    expect(tones).toEqual(new Set(["pen", "good", "pencil"]));
  });

  it("captions the move in notation and is keyboard-reachable", () => {
    const { container } = render(<ExemplarBoard {...props} />);
    expect(container.textContent).toContain("4. Nxe5? Better was Nxd4. Win chance 52% to 36%.");
    expect(exemplarCaption(props)).toBe("4. Nxe5? Better was Nxd4. Win chance 52% to 36%.");
    expect(screen.getByRole("group", { name: "Position before 4. Nxe5" })).toBeInTheDocument();
    expect(container.textContent).not.toMatch(BAD);
  });

  it("leaves out what it does not have", () => {
    const text = exemplarCaption({
      fen: fen.replace(" w ", " b "),
      played: { san: "Nf6", uci: "g8f6" },
      best: null,
      refutation: null,
      caption: { judgement: null, winBefore: null, winAfter: null },
    });
    expect(text).toBe("4… Nf6.");
  });
});
