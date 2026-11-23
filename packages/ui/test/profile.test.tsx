import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  bandComparisonText,
  ExemplarBoard,
  exemplarCaption,
  formatKpiValue,
  KpiTrendGrid,
  type KpiTrendItem,
  kpiDomain,
  kpiSummary,
  kpiYScale,
  needMoreText,
  WeaknessRow,
  type WeaknessRowProps,
  weaknessRateText,
  weaknessScaleMax,
} from "../src/profile/index.js";

afterEach(cleanup);

const BAD = /NaN|undefined|Infinity|null/;
const NO_BENCHMARK = "Not compared yet: no benchmark for players at your level yet";

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
    ).toBe("Estimated 4.2% of chances (80% range 3.1% to 5.6%), from 38 of 902");
  });
});

describe("WeaknessRow", () => {
  it("links to the detail and shows the rate with its counts", () => {
    const { container } = render(<WeaknessRow {...row} />);
    expect(screen.getByRole("link", { name: /Hanging pieces/ })).toHaveAttribute(
      "href",
      "/app/weaknesses/W1",
    );
    expect(container.textContent).toContain("38 times in 902 chances");
    expect(container.textContent).toContain("Active");
    // The posterior and its interval are labelled on screen, not only for screen readers.
    const visible = (el: Element): string =>
      el.classList.contains("sr-only")
        ? ""
        : el.children.length === 0
          ? (el.textContent ?? "")
          : [...el.childNodes]
              .map((n) => (n.nodeType === 3 ? n.textContent : visible(n as Element)))
              .join("");
    expect(visible(container)).toContain("Estimated % of chances");
    expect(visible(container)).toContain("80% range 3.1% to 5.6%");
    // The interval clears the median: the comparison says so, and names the reference group.
    expect(container.textContent).toContain(
      "Players in your rating band: 2.1%, lower than your whole likely range",
    );
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
    expect(container.textContent).toContain("Early estimate");
    expect(container.textContent).not.toMatch(/rating band/i);
    // No settled-looking trend under the minimum: the pending wording instead.
    expect(container.querySelector("svg[role='img']")).toBeNull();
    expect(container.textContent).toContain("Weekly trend after 180 more chances");
    expect(container.querySelector("[data-part='rate-strip']")).toBeNull();
    expect(container.textContent).not.toMatch(BAD);
  });

  it("shows no estimate at all with no chances yet", () => {
    const { container } = render(
      <WeaknessRow
        {...row}
        rate={{ mean: 0.05, low: 0.001, high: 0.4 }}
        evidence={{ events: 0, chances: 0, sufficient: false, needMore: 220 }}
        trend={[]}
      />,
    );
    expect(container.textContent).toContain("No chances yet");
    expect(container.textContent).not.toContain("5.0%");
    expect(container.textContent).toContain("0 times in 0 chances");
    expect(container.textContent).not.toMatch(BAD);
  });

  it("names what it counts and the band when given", () => {
    const { container } = render(
      <WeaknessRow
        {...row}
        counts={{ event: "errors", chance: "rook-endgame moves" }}
        band={{ median: 0.08, label: "Players rated 1400–1600" }}
      />,
    );
    expect(container.textContent).toContain("Estimated % of rook-endgame moves");
    expect(container.textContent).toContain("38 errors in 902 rook-endgame moves");
    expect(container.textContent).toContain(
      "Players rated 1400–1600: 8.0%, higher than your whole likely range",
    );
  });

  // Bug hunt I2: a high end of 98% padded by 10% labelled the scale "110.0%". A rate is a share.
  it("never puts the interval scale past 100%", () => {
    const high = { rate: { mean: 0.9, low: 0.8, high: 0.98 }, band: { median: null } };
    expect(weaknessScaleMax([high])).toBe(1);
    const { container } = render(<WeaknessRow {...row} {...high} />);
    expect(container.querySelector("[data-part='scale-max']")?.textContent).toBe("100.0%");
  });

  it("rows in a list share one interval scale", () => {
    const a = { rate: { mean: 0.042, low: 0.031, high: 0.056 }, band: { median: 0.021 } };
    const b = { rate: { mean: 0.118, low: 0.084, high: 0.152 }, band: { median: 0.11 } };
    const max = weaknessScaleMax([a, b]);
    expect(max).toBeGreaterThanOrEqual(0.152);
    const { container } = render(
      <>
        <WeaknessRow {...row} {...a} scaleMax={max} />
        <WeaknessRow {...row} {...b} scaleMax={max} />
      </>,
    );
    const widths = [...container.querySelectorAll<HTMLElement>("[data-part='interval']")].map(
      (el) => el.style.width,
    );
    expect(widths).toHaveLength(2);
    expect(widths[0]).not.toBe(widths[1]);
    // The scale's top end is labelled, the same on every row.
    const ends = [...container.querySelectorAll("[data-part='scale-max']")].map(
      (e) => e.textContent,
    );
    expect(ends).toHaveLength(2);
    expect(ends[0]).toBe(ends[1]);
  });

  it("says when there is no band reference, never a zero", () => {
    const { container } = render(<WeaknessRow {...row} band={{ median: null }} onset={null} />);
    expect(container.textContent).toContain(NO_BENCHMARK);
    expect(container.querySelector("time")).toBeNull();
    expect(container.textContent).not.toMatch(BAD);
  });

  // Bug hunt I1: with no benchmark Rust stores `not_an_issue`, which is no claim at all. The row
  // says it is not compared yet and still shows its own rate, range and counts.
  it("never calls an uncompared weakness 'not an issue'", () => {
    const { container } = render(
      <WeaknessRow {...row} state="not_an_issue" band={{ median: null }} compared={false} />,
    );
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/not an issue/i);
    expect(text).toContain("Not compared yet");
    expect(text).toContain(NO_BENCHMARK);
    expect(text).toContain("4.2%");
    expect(text).toContain("80% range 3.1% to 5.6%");
    expect(text).toContain("38 times in 902 chances");
  });

  it("names the estimate's window beside its counts", () => {
    const { container } = render(<WeaknessRow {...row} window="in the last 6 months" />);
    expect(container.textContent).toContain("38 times in 902 chances in the last 6 months");
    expect(container.textContent).toContain("from 38 of 902 in the last 6 months");
  });
});

describe("bandComparisonText", () => {
  const r = { low: 0.03, high: 0.06 };
  it("says lower only when the median is under the whole interval", () => {
    expect(bandComparisonText(r, 0.02)).toBe(
      "Players in your rating band: 2.0%, lower than your whole likely range",
    );
  });
  it("says higher only when the median is over the whole interval", () => {
    expect(bandComparisonText(r, 0.07, "Players rated 1200–1400")).toBe(
      "Players rated 1200–1400: 7.0%, higher than your whole likely range",
    );
  });
  it("says inside when the interval contains the median, ends included", () => {
    expect(bandComparisonText(r, 0.045)).toBe(
      "Players in your rating band: 4.5%, inside your likely range",
    );
    expect(bandComparisonText(r, 0.03)).toContain("inside your likely range");
    expect(bandComparisonText(r, 0.06)).toContain("inside your likely range");
  });
  it("says what a pooled peer rate is pooled over", () => {
    expect(
      bandComparisonText(
        r,
        0.096,
        "Players you've faced, rated about 650–950",
        "pooled over 65 of their chances",
      ),
    ).toBe(
      "Players you've faced, rated about 650–950: 9.6% (pooled over 65 of their chances), higher than your whole likely range",
    );
  });
  // ADR 0084 (bug hunt N10): the owner's rapid "Allowed mate" was active at confidence 0.81
  // while the row read only "inside your likely range". The row now says what the state says.
  const allowed = { low: 0.00293, high: 0.00782 };
  const faced = "Players you've faced, rated about 667–967";
  const pooled = "pooled over 1,484 of their chances";
  it("names a probable gap inside the range with the stored confidence", () => {
    expect(bandComparisonText(allowed, 0.003498, faced, pooled, 0.81)).toBe(
      "Players you've faced, rated about 667–967: 0.3% (pooled over 1,484 of their chances), inside your likely range, though yours is probably higher (81% likely)",
    );
  });
  it("says only inside below the probable threshold or without a confidence", () => {
    const plain =
      "Players you've faced, rated about 667–967: 0.3% (pooled over 1,484 of their chances), inside your likely range";
    expect(bandComparisonText(allowed, 0.003498, faced, pooled, 0.547)).toBe(plain);
    expect(bandComparisonText(allowed, 0.003498, faced, pooled)).toBe(plain);
    expect(bandComparisonText(allowed, 0.003498, faced, pooled, Number.NaN)).toBe(plain);
    expect(bandComparisonText(allowed, 0.003498, faced, pooled, 1.5)).toBe(plain);
  });
  it("keeps lower and higher when the interval clears the median", () => {
    expect(bandComparisonText(r, 0.02, undefined, undefined, 0.99)).toBe(
      "Players in your rating band: 2.0%, lower than your whole likely range",
    );
  });
  it("says when there is no reference", () => {
    expect(bandComparisonText(r, null)).toBe(NO_BENCHMARK);
    expect(bandComparisonText(r, Number.NaN)).toBe(NO_BENCHMARK);
  });
});

describe("needMoreText", () => {
  it("counts the shortfall, singular for one, plain when unknown", () => {
    expect(needMoreText(180)).toBe("180 more chances needed");
    expect(needMoreText(1)).toBe("1 more chance needed");
    expect(needMoreText(0)).toBe("More chances needed");
    expect(needMoreText(null)).toBe("More chances needed");
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

  it("scales to the weeks on the line, not a thin week, and clips the thin week's marks (M4)", () => {
    // Weekly weakness rates: two weeks at the minimum near 10%, and one 10-chance week at 30%
    // whose wide interval once stretched the axis to 75%.
    const rates: KpiTrendItem = {
      metric: "weakness:W1",
      label: "Weekly rate",
      unit: "rate",
      nUnit: { one: "chance", many: "chances" },
      points: [
        { week: "2026-11-02", value: 0.1, low: 0.06, high: 0.15, n: 40 },
        { week: "2026-11-09", value: 0.3, low: 0.1, high: 0.62, n: 10 },
        { week: "2026-11-16", value: 0.12, low: 0.08, high: 0.17, n: 45 },
      ],
    };
    const [lo, hi] = kpiDomain(rates, 25);
    expect(hi).toBeLessThan(0.3);
    expect(hi).toBeGreaterThanOrEqual(0.17);
    expect(lo).toBeLessThanOrEqual(0.06);
    // Without a minimum, every week counts, as before.
    expect(kpiDomain(rates)[1]).toBeGreaterThanOrEqual(0.62);
    const { container } = render(<KpiTrendGrid items={[rates]} range={range} minN={25} />);
    expect(container.textContent).not.toContain("75");
    const hollow = container.querySelector("[data-part='hollow']");
    const clip = hollow?.closest("[clip-path]");
    expect(clip).not.toBeNull();
    const id = clip?.getAttribute("clip-path")?.match(/^url\(#(.+)\)$/)?.[1];
    expect(id && container.querySelector(`clipPath[id="${id}"]`)).toBeTruthy();
    // The band of the thin week is clipped to the plot too.
    const bandClip = container.querySelector("[data-part='band']")?.closest("[clip-path]");
    expect(bandClip?.getAttribute("clip-path")).toBe(clip?.getAttribute("clip-path"));
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

  it("a single point on the line is a dot, and its band a bar between low and high", () => {
    const it0 = item([1.2, null, null]);
    const { container } = render(<KpiTrendGrid items={[it0]} range={range} minN={5} />);
    const seg = container.querySelectorAll("[data-part='segment']");
    expect(seg).toHaveLength(1);
    expect(seg[0]?.getAttribute("d")).toMatch(/h0$/);
    const d = container.querySelector("[data-part='band']")?.getAttribute("d") ?? "";
    const y = kpiYScale(it0);
    const ys = [...d.matchAll(/[ML]\s*([\d.-]+)[ ,]([\d.-]+)/g)].map((m) => Number(m[2]));
    expect(ys).toHaveLength(4);
    expect(ys[0]).toBeCloseTo(y(1.5), 6);
    expect(ys[3]).toBeCloseTo(y(0.9), 6);
  });

  it("a value with n = 0 is hollow and named, and minN = 0 puts every value on the line", () => {
    const it0 = item([1.2, 1.1, 0.8], [{}, { n: 0 }, {}]);
    const { container } = render(<KpiTrendGrid items={[it0]} range={range} minN={5} />);
    expect(container.querySelectorAll("[data-part='hollow']")).toHaveLength(1);
    expect(kpiSummary(it0, 5)).toContain("1.10 in week of 9 Nov 2026, 0 games, below");
    cleanup();
    const { container: c2 } = render(<KpiTrendGrid items={[it0]} range={range} minN={0} />);
    expect(c2.querySelectorAll("[data-part='hollow']")).toHaveLength(0);
    expect(c2.querySelectorAll("[data-part='segment']")).toHaveLength(1);
  });

  it("the key explains every mark, in the item's unit", () => {
    const { container } = render(
      <KpiTrendGrid
        items={[item([1.2, null, 0.8])]}
        range={range}
        minN={20}
        nUnit={{ one: "chance", many: "chances" }}
        events={[{ date: "2026-11-09", label: "Report" }]}
      />,
    );
    const key = screen.getByRole("list", { name: "Key" }).textContent ?? "";
    expect(key).toContain("20 or more chances");
    expect(key).toContain("Fewer than 20 chances");
    expect(key).toContain("A week on its own");
    expect(key).toContain("Gap: no chances that week");
    expect(key).toContain("Stored 80% range");
    expect(key).toContain("Report");
    expect(container.querySelector("h3")).not.toBeNull();
  });

  it("takes the host's heading level and dates its headline", () => {
    const { container } = render(
      <KpiTrendGrid items={[item([1.2, 1.0, 0.8])]} range={range} minN={5} headingLevel={2} />,
    );
    expect(container.querySelector("h2")?.textContent).toBe("Blunders per game");
    expect(container.querySelector("figcaption time")).toHaveAttribute("dateTime", "2026-11-16");
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
    expect(formatKpiValue(0.5, null)).toBe("0.50");
    expect(formatKpiValue(3.256, "moves")).toBe("3.26 moves");
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
    // The win-chance pair is kept on one line with no-break spaces.
    expect(container.textContent?.replace(/\u00a0/g, " ")).toContain(
      "4. Nxe5? Better was Nxd4. Win chance 52% to 36%.",
    );
    // The reply is named, not only drawn.
    expect(container.textContent).toContain("Their reply, 4… Qg5");
    expect(exemplarCaption(props)).toBe("4. Nxe5? Better was Nxd4. Win chance 52% to 36%.");
    expect(screen.getByRole("group", { name: "Position before 4. Nxe5" })).toBeInTheDocument();
    expect(container.textContent).not.toMatch(BAD);
  });

  // Owner request: the move and its reply are a sequence, numbered 1 and 2 on the board and in
  // the key; the better move is the alternative, unnumbered.
  it("numbers the move and its reply, in order, on the board and in the key", () => {
    const { container } = render(<ExemplarBoard {...props} />);
    const steps = [...container.querySelectorAll("[data-mark='step']")];
    expect(steps.map((g) => [g.textContent, g.getAttribute("data-tone")])).toEqual([
      ["1", "pen"],
      ["2", "pencil"],
    ]);
    const key = screen.getByRole("list", { name: "Arrows" });
    expect([...key.querySelectorAll("li")].map((li) => li.textContent)).toEqual([
      "1. Played, 4. Nxe5",
      "2. Their reply, 4… Qg5",
      "Better, Nxd4",
    ]);
  });

  it("draws only the arrows it has, and ignores a malformed move", () => {
    const { container } = render(
      <ExemplarBoard
        {...props}
        best={null}
        refutation="zz99"
        caption={{ ...props.caption, judgement: null }}
      />,
    );
    const tones = [...container.querySelectorAll("[data-mark='arrow']")].map((a) =>
      a.getAttribute("data-tone"),
    );
    expect(new Set(tones)).toEqual(new Set(["pen"]));
    expect(container.textContent).not.toContain("Better");
    expect(container.textContent).not.toContain("Their reply");
    expect(container.textContent).not.toMatch(BAD);
  });

  it("clamps win chances to 0 to 100", () => {
    expect(
      exemplarCaption({ ...props, caption: { judgement: null, winBefore: 104, winAfter: -3 } }),
    ).toBe("4. Nxe5. Better was Nxd4. Win chance 100% to 0%.");
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

describe("WeaknessRow colour split (T3, ADR 0082)", () => {
  it("shows one colour without a state badge: states are called on every game", () => {
    const { container } = render(<WeaknessRow {...row} side="white" />);
    expect(container.textContent).not.toContain("Active");
    expect(container.textContent).toContain("Your White games");
    expect(container.textContent).toContain("38 times in 902 chances");
  });

  it("says no colour benchmark, never the both one, when the colour has no peer row", () => {
    const { container } = render(
      <WeaknessRow {...row} band={{ median: null }} compared={false} side="black" />,
    );
    expect(container.textContent).toContain("No colour benchmark");
    expect(container.textContent).not.toContain("Not compared yet");
  });

  it("keeps the early read per side", () => {
    const { container } = render(
      <WeaknessRow
        {...row}
        side="black"
        evidence={{ events: 2, chances: 12, sufficient: false, needMore: 18 }}
      />,
    );
    expect(container.textContent).toContain("Early read");
    expect(container.textContent).not.toContain("Active");
  });

  it("in Both mode lists each colour's rate with its n under the row", () => {
    const { container } = render(
      <WeaknessRow
        {...row}
        sides={{
          white: { mean: 0.051, chances: 480, sufficient: true },
          black: { mean: 0.032, chances: 12, sufficient: false },
        }}
      />,
    );
    const split = container.querySelector("[data-part='sides']");
    expect(split).toHaveTextContent("White 5.1% of 480 chances");
    expect(split).toHaveTextContent("Black early read, 12 chances");
    expect(container.textContent).toContain("Active");
    expect(container.textContent).not.toMatch(BAD);
  });
});
