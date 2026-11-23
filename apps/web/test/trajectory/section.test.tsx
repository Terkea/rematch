// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { TrajectoryCard } from "../../src/app/(app)/app/_trajectory/card.js";
import { TrajectorySection } from "../../src/app/(app)/app/_trajectory/section.js";
import { trajectoryView } from "../../src/app/(app)/app/_trajectory/view.js";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { computeTrajectory } from "../../src/server/trajectory/model.js";

// The trajectory on Progress (its first section) and its compact card for the Overview (F6).
// Synthetic games only (B15).

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
});

const BAD = /NaN|undefined|Infinity|→/;
const DAY = 86_400_000;
const START = Date.UTC(2026, 0, 1);

function view() {
  const ratings = Array.from({ length: 100 }, (_, i) =>
    i <= 40 ? 800 + i * 7 : 1080 - (i - 40) * 4,
  );
  const games = ratings.map((rating, i) => ({
    id: `g${i}`,
    playedAt: new Date(START + i * DAY),
    rating,
  }));
  const stats: GameStatRow[] = games.map((g, i) => ({
    id: g.id,
    playedAt: g.playedAt,
    color: "white",
    result: i % 2 === 0 ? "win" : "loss",
    analysed: true,
    accuracy: i >= 70 ? 62 + (i % 3) : 78 + (i % 3),
    blunders: 1,
    mistakes: 1,
    inaccuracies: 1,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
  }));
  const t = computeTrajectory({
    games,
    current: { rating: 816, at: new Date(START + 100 * DAY) },
    stats,
    weaknesses: [],
    seed: "s",
  });
  return trajectoryView(t, "rapid");
}

describe("TrajectorySection", () => {
  it("leads with the headline, then the chart and the then | now table", () => {
    const { container } = render(<TrajectorySection view={view()} />);
    const section = screen.getByRole("region", { name: "Rating trajectory" });
    expect(
      within(section).getByText(/peaked at 1,080 in February 2026 and is 816 now/),
    ).toBeTruthy();
    expect(within(section).getByText(/^Since then: accuracy/)).toBeTruthy();
    const table = container.querySelector('[data-slot="then-now"]');
    expect(table).not.toBeNull();
    expect(
      within(table as HTMLElement).getByText("Around your peak", { selector: "dt" }),
    ).toBeTruthy();
    expect(container.textContent).not.toMatch(BAD);
  });

  it("says how many more rated games it needs, with no chart", () => {
    const t = computeTrajectory({
      games: [{ id: "a", playedAt: new Date(START), rating: 900 }],
      current: null,
      stats: [],
      weaknesses: [],
      seed: "s",
    });
    const { container } = render(<TrajectorySection view={trajectoryView(t, "rapid")} />);
    expect(screen.getByText(/appears after 20 rated rapid games: 19 more to go/)).toBeTruthy();
    expect(container.querySelector('[data-slot="then-now"]')).toBeNull();
  });
});

describe("TrajectoryCard", () => {
  // The overview opens on it (ADR 0092): where you are now, the peak against now, and Progress.
  it("leads the overview with the current rating, the headline and a link to Progress", () => {
    const { container } = render(<TrajectoryCard view={view()} href="/app/progress?tc=rapid" />);
    const card = screen.getByRole("region", { name: "Where you are" });
    const now = card.querySelector('[data-part="rating-now"]');
    expect(now?.textContent).toBe("816");
    expect(card).toHaveTextContent("Rapid rating now");
    expect(within(card).getByText(/peaked at 1,080/)).toBeTruthy();
    expect(card).toHaveTextContent("Based on 100 rated rapid games.");
    const link = within(card).getByRole("link", { name: "See what changed" });
    expect(link.getAttribute("href")).toBe("/app/progress?tc=rapid");
    expect(container.textContent).not.toMatch(BAD);
  });

  it("says how many more rated games it needs below the minimum, with no number", () => {
    const t = computeTrajectory({
      games: [{ id: "a", playedAt: new Date(START), rating: 900 }],
      current: null,
      stats: [],
      weaknesses: [],
      seed: "s",
    });
    const { container } = render(
      <TrajectoryCard view={trajectoryView(t, "rapid")} href="/app/progress?tc=rapid" />,
    );
    const card = screen.getByRole("region", { name: "Where you are" });
    expect(card).toHaveTextContent(/appears after 20 rated rapid games: 19 more to go/);
    expect(card.querySelector('[data-part="rating-now"]')).toBeNull();
    expect(within(card).getByRole("link", { name: "See your progress" })).toHaveAttribute(
      "href",
      "/app/progress?tc=rapid",
    );
    expect(container.textContent).not.toMatch(BAD);
  });
});
