// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { compareView } from "../../src/app/(app)/app/_filters/compare-view.js";
import { AccuracySection } from "../../src/app/(app)/app/_overview/accuracy-section.js";
import { OverviewEmpty } from "../../src/app/(app)/app/_overview/empty-state.js";
import { JudgementsSection } from "../../src/app/(app)/app/_overview/judgements-section.js";
import { RecentSection } from "../../src/app/(app)/app/_overview/recent-section.js";
import { OverviewSheet } from "../../src/app/(app)/app/_overview/sheet.js";
import { StatusLine } from "../../src/app/(app)/app/_overview/status-line.js";
import { TimeClassSection } from "../../src/app/(app)/app/_overview/time-class-section.js";
import { TimeClassSwitch } from "../../src/app/(app)/app/_overview/time-class-switch.js";
import {
  chooseTimeClass,
  parseTimeClass,
  statusText,
  toOverviewView,
} from "../../src/app/(app)/app/_overview/view.js";
import type { TimeClass } from "../../src/server/games/types.js";
import { getOverview } from "../../src/server/overview/index.js";
import type { GameStatRow } from "../../src/server/stats/game-stats.js";
import { comparePeriodRows } from "../../src/server/stats/period-compare.js";
import {
  resetGameTables,
  type SyntheticGame,
  seedAccount,
  seedSyntheticGame,
} from "../helpers/games.js";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session.js";

// The overview's sections rendered from Overview objects that Task 2 builds against seeded
// Postgres. Synthetic games only (B15).

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

const db = webTestDb();
let userId: string;

beforeAll(async () => {
  await resetAuthTables(db);
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

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
});

const day = (i: number) => new Date(Date.UTC(2026, 0, 1 + i, 12)).toISOString();

async function linked(username = "someplayer") {
  return (await seedAccount(db, { userId, username })).accountId;
}

async function seed(
  accountId: string,
  n: number,
  over: (i: number) => Partial<SyntheticGame> = () => ({}),
) {
  for (let i = 0; i < n; i++) {
    await seedSyntheticGame(db, accountId, {
      playedAt: day(i),
      timeClass: "rapid",
      result: i % 3 === 0 ? "0-1" : "1-0",
      accuracy: 55 + (i % 9) * 3,
      counts: { blunders: i % 3, mistakes: i % 2, inaccuracies: 1 + (i % 4) },
      ...over(i),
    });
  }
}

async function viewFor(tc: TimeClass | null = null) {
  return toOverviewView(await getOverview(db, userId, { timeClass: tc }), tc);
}

const BAD = /NaN|undefined|Infinity/;

describe("overview page sections", () => {
  // Critic 4: an active comparison sits right under the filter bar, not 1,100 px down on a phone.
  it("puts the comparison right under the filter bar when one is active", async () => {
    await seed(await linked(), 30);
    const view = await viewFor();
    const stat = (n: number): GameStatRow[] =>
      Array.from({ length: n }, (_, i) => ({
        id: `g${i}`,
        playedAt: new Date(Date.UTC(2026, 9, 1 + (i % 28))),
        color: "white",
        result: "win",
        analysed: true,
        accuracy: 60 + (i % 3),
        blunders: 1,
        mistakes: 1,
        inaccuracies: 1,
        phaseAccuracy: { opening: null, middlegame: null, endgame: null },
      }));
    const compare = compareView({
      periods: {
        a: { kind: "days", from: "2026-09-01", to: "2026-09-30" },
        b: { kind: "days", from: "2026-10-01", to: "2026-10-31" },
      },
      spans: { a: null, b: null },
      comparison: comparePeriodRows(stat(20), stat(20), "t"),
    });
    const { container } = render(
      <OverviewSheet
        view={view}
        reveal={false}
        filterBar={<div data-testid="bar" />}
        compare={compare}
      />,
    );
    const order = [...container.querySelectorAll("[data-testid='bar'], [data-slot]")].map(
      (el) => el.getAttribute("data-slot") ?? "bar",
    );
    expect(order.slice(0, 2)).toEqual(["bar", "compare"]);
  });

  it("renders a one-game account without NaN", async () => {
    await seed(await linked(), 1);
    const view = await viewFor();
    const { container } = render(<OverviewSheet view={view} reveal={false} />);
    expect(container.innerHTML).not.toMatch(BAD);
    expect(container.textContent).toContain("1 game analysed");
    expect(container.textContent).toContain("Need 9 more games");
  });

  it("states n and the shortfall once per section in a one-game account, and plots the game", async () => {
    await seed(await linked(), 1);
    const view = await viewFor();
    const { container } = render(<OverviewSheet view={view} reveal={false} />);
    const visible = (root: Element, text: string) =>
      [...root.querySelectorAll("p, span, h2, h3")].filter(
        (el) =>
          el.children.length === 0 &&
          (el.textContent ?? "").includes(text) &&
          !el.closest(".sr-only") &&
          !el.classList.contains("sr-only"),
      ).length;
    for (const id of ["judgements-heading", "tc-heading"]) {
      const section = container.querySelector(`#${id}`)?.closest("section") as Element;
      expect(visible(section, "Based on 1")).toBeLessThanOrEqual(1);
      expect(visible(section, "Need 9 more games")).toBe(1);
    }
    // The one game is plotted (minPoints 1; @ci/ui's test draws its dot), and the line waits.
    expect(view.accuracy.points).toHaveLength(1);
    expect(container.textContent).not.toContain("Not enough games yet");
    expect(container.textContent).toContain("The line starts once you have 20 analysed games.");
  });

  it("renders a rich account without NaN, with every section", async () => {
    const acc = await linked();
    await seed(acc, 24);
    await seed(acc, 12, (i) => ({ playedAt: day(100 + i), timeClass: "blitz" }));
    const view = await viewFor();
    const { container } = render(<OverviewSheet view={view} reveal />);
    expect(container.innerHTML).not.toMatch(BAD);
    for (const h of [
      "Overview",
      "Accuracy over time",
      "Mistakes per game",
      "By time control",
      "Recent games",
      "What to work on",
    ]) {
      expect(screen.getByRole("heading", { name: h })).toBeInTheDocument();
    }
    expect(container.querySelector('[data-reveal="first"]')).not.toBeNull();
  });

  it("states the status line for a partial analysis", async () => {
    const acc = await linked();
    await seed(acc, 4);
    for (const status of [
      "pending",
      "pending",
      "analysing",
      "failed",
      "skipped",
      "skipped",
    ] as const) {
      await seedSyntheticGame(db, acc, {
        playedAt: day(50),
        timeClass: "rapid",
        result: "1-0",
        status,
      });
    }
    const view = await viewFor();
    render(<StatusLine header={view.header} />);
    expect(
      screen.getByText(
        "4 of 10 games analysed. 3 waiting for analysis. 1 game couldn't be analysed. 2 games skipped (variants or too short).",
      ),
    ).toBeInTheDocument();
  });

  it("uses the brief's copy for 412 of 500", () => {
    expect(statusText({ total: 500, analysed: 412, waiting: 88, failed: 3, skipped: 2 })).toEqual([
      "412 of 500 games analysed.",
      "88 waiting for analysis.",
      "3 games couldn't be analysed.",
      "2 games skipped (variants or too short).",
    ]);
    expect(statusText({ total: 500, analysed: 412, waiting: 88, failed: 0, skipped: 0 })).toEqual([
      "412 of 500 games analysed.",
      "88 waiting for analysis.",
    ]);
    expect(statusText({ total: 500, analysed: 500, waiting: 0, failed: 0, skipped: 0 })).toEqual([
      "500 games analysed",
    ]);
  });

  it("names the account, unverified, with its last import", async () => {
    await seed(await linked("someplayer"), 2);
    const view = await viewFor();
    render(<StatusLine header={view.header} />);
    expect(screen.getByText("chess.com someplayer")).toBeInTheDocument();
    expect(screen.getByText(/Unverified/)).toBeInTheDocument();
  });
});

describe("overview empty states", () => {
  it("asks to connect chess.com when nothing is linked", async () => {
    const view = await viewFor();
    const { container } = render(<OverviewSheet view={view} reveal={false} />);
    expect(
      screen.getByText("Connect a chess.com account to see your games here."),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Connect chess.com" })).toHaveAttribute(
      "href",
      "/app/accounts",
    );
    expect(container.querySelector("[data-chart]")).toBeNull();
    expect(container.innerHTML).not.toMatch(BAD);
    // No games at all: no "0 games analysed" status line.
    expect(container.textContent).not.toContain("0 games analysed");
  });

  it("shows the importing placeholder for a link with no games", async () => {
    await linked();
    const view = await viewFor();
    expect(view.empty).toEqual({ kind: "no-games" });
    render(<OverviewEmpty state={view.empty ?? { kind: "no-links" }} />);
    expect(screen.getAllByText(/Importing/).length).toBeGreaterThan(0);
  });

  it("shows analysing 0 of N when no game is analysed yet", async () => {
    const acc = await linked();
    for (let i = 0; i < 3; i++) {
      await seedSyntheticGame(db, acc, {
        playedAt: day(i),
        timeClass: "rapid",
        result: "1-0",
        status: "pending",
      });
    }
    const view = await viewFor();
    expect(view.empty).toEqual({ kind: "none-analysed", total: 3 });
    const { container } = render(<OverviewSheet view={view} reveal={false} />);
    expect(container.textContent).toContain("Analysing 0 / 3 games");
    expect(container.querySelector("[data-chart]")).toBeNull();
  });
});

describe("time control", () => {
  it("lists All and each analysed class with its count, as links with aria-current", async () => {
    const acc = await linked();
    await seed(acc, 3);
    await seed(acc, 2, (i) => ({ playedAt: day(40 + i), timeClass: "blitz" }));
    const view = await viewFor("rapid");
    render(<TimeClassSwitch options={view.switch} />);
    const nav = screen.getByRole("navigation", { name: "Time control" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((l) => l.textContent)).toEqual(["Rapid 3", "Blitz 2", "All 5"]);
    expect(within(nav).getByRole("link", { name: "Rapid 3" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Rapid 3" })).toHaveAttribute(
      "href",
      "/app?tc=rapid",
    );
    // Rapid is the default (I8), so All is asked for by name.
    expect(within(nav).getByRole("link", { name: "All 5" })).toHaveAttribute("href", "/app?tc=all");
    expect(within(nav).getByRole("link", { name: "All 5" })).not.toHaveAttribute("aria-current");
  });

  it("defaults to rapid when there is rapid, and tc=all shows every class (I8)", () => {
    const have: TimeClass[] = ["blitz", "rapid"];
    expect(chooseTimeClass(undefined, have)).toBe("rapid");
    expect(chooseTimeClass("bogus", have)).toBe("rapid");
    expect(chooseTimeClass("blitz", have)).toBe("blitz");
    expect(chooseTimeClass("all", have)).toBeNull();
    expect(chooseTimeClass(undefined, ["daily", "blitz"])).toBe("blitz");
    expect(chooseTimeClass(undefined, [])).toBeNull();
  });

  it("falls back to All for an unknown or unanalysed class", () => {
    const have: TimeClass[] = ["blitz", "rapid"];
    expect(parseTimeClass("rapid", have)).toBe("rapid");
    expect(parseTimeClass("bogus", have)).toBeNull();
    expect(parseTimeClass("bullet", have)).toBeNull();
    expect(parseTimeClass(["rapid", "blitz"], have)).toBe("rapid");
    expect(parseTimeClass(undefined, have)).toBeNull();
  });

  it("scopes the headline and rates to the class, with the class as the basis", async () => {
    const acc = await linked();
    await seed(acc, 12);
    await seed(acc, 3, (i) => ({ playedAt: day(40 + i), timeClass: "blitz" }));
    const view = await viewFor("blitz");
    const { container } = render(<JudgementsSection judgements={view.judgements} />);
    expect(container.textContent).toContain("Need 7 more games");
    expect(container.textContent).toContain("Based on 3 blitz games");
  });
});

describe("sections", () => {
  it("shows three per-game rows with one stated basis, then the last games' columns", async () => {
    await seed(await linked(), 14);
    const view = await viewFor();
    const { container } = render(<JudgementsSection judgements={view.judgements} />);
    for (const label of ["Blunders", "Mistakes", "Inaccuracies"]) {
      expect(screen.getByRole("heading", { name: label })).toBeInTheDocument();
    }
    expect(container.textContent).toContain("Across 14 analysed games.");
    // Audit A10: every row says what it counts, mistakes and inaccuracies too.
    for (const name of ["blunders per game", "mistakes per game", "inaccuracies per game"]) {
      expect(screen.getByRole("button", { name: `What is ${name}?` })).toBeInTheDocument();
    }
    expect(
      screen.getByRole("heading", { name: "Your last 14 analysed games" }),
    ).toBeInTheDocument();
    // n stated once: the rows keep their basis for screen readers only.
    for (const b of screen.getAllByText("Based on 14 analysed games"))
      expect(b).toHaveClass("sr-only");
    expect(container.textContent).toContain("Lower is better");
    // T13 critic I3: each sparkline's ends name their week and its games.
    const ends = [...container.querySelectorAll("[data-part='trend-ends']")];
    expect(ends).toHaveLength(3);
    for (const e of ends) {
      expect(e.textContent).toMatch(/First week \d+\.\d \(\d+ games?\)/);
      expect(e.textContent).toMatch(/Latest week \d+\.\d \(\d+ games?\)/);
    }
  });

  it("splits results by time control with a score and accuracy per class", async () => {
    const acc = await linked();
    await seed(acc, 12);
    const view = await viewFor();
    const { container } = render(<TimeClassSection section={view.byTimeClass} />);
    expect(container.textContent).toMatch(/\d+ wins?, \d+ draws?, \d+ loss(es)?/);
    expect(screen.getByRole("heading", { name: "Rapid score" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Rapid accuracy" })).toBeInTheDocument();
    expect(container.textContent).toContain("Score counts a draw as half a win.");
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("lists six recent games, each a link to its review", async () => {
    await seed(await linked(), 8);
    const view = await viewFor();
    render(<RecentSection rows={view.recent} />);
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(6);
    for (const l of links) expect(l.getAttribute("href")).toMatch(/^\/app\/games\/[0-9a-f-]+$/);
    expect(links[0]?.getAttribute("aria-label")).toMatch(/^\d+ \w+ 2026, (win|loss|draw) v /);
  });

  it("shows each recent row as a link, with the opponent's rating kept whole", async () => {
    await seed(await linked(), 2, () => ({ opponentRating: 1612 }));
    const view = await viewFor();
    const { container } = render(<RecentSection rows={view.recent} />);
    const row = container.querySelector("tbody tr") as Element;
    // A visible cue: the date is underlined and the row ends in a chevron.
    expect(row.querySelector("a")?.className).toMatch(/underline/);
    expect(row.querySelector('[data-part="row-chevron"]')).not.toBeNull();
    // The rating is its own element and the opponent cell may wrap: nothing is truncated.
    expect(within(row as HTMLElement).getByText("1612")).toBeInTheDocument();
    expect(container.querySelector(".truncate")).toBeNull();
  });

  it("captions the trend and says when it shows only the latest games", async () => {
    await seed(await linked(), 3);
    const view = await viewFor();
    render(<AccuracySection accuracy={view.accuracy} drawIn={false} />);
    expect(
      screen.getByText(
        "Each dot is a game. The line is your average over the last 20 games, with an 80% band.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Showing your latest 500 analysed games.")).toBeNull();
    expect(
      toOverviewView(await getOverview(db, userId, { timeClass: null }), null).accuracy.capped,
    ).toBe(false);
  });

  it("keeps the 'What to work on' slot empty until the profile exists", async () => {
    await seed(await linked(), 12);
    const view = await viewFor();
    render(<OverviewSheet view={view} reveal={false} />);
    const slot = screen.getByRole("heading", { name: "What to work on" }).closest("section");
    expect(slot?.textContent).not.toMatch(/\d/);
    expect(slot?.querySelector("a")).toBeNull();
  });
});

describe("client boundary", () => {
  it("gives the trend only plain, rendered fields: no opponents, accounts or raw rows", async () => {
    const acc = await linked();
    await seed(acc, 12, () => ({ opponentRating: 1611 }));
    const view = await viewFor();
    const json = JSON.stringify(view.accuracy);
    expect(JSON.parse(json)).toStrictEqual(view.accuracy);
    expect(json).not.toContain(acc);
    // Game ids are random uuids, which may contain "1611"; strip them before looking for leaks.
    const text = json.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, "");
    expect(text).not.toMatch(/opponent|1611|someplayer|accountId|openingName/i);
    for (const p of view.accuracy.points) {
      expect(Object.keys(p).sort()).toEqual([
        "accuracy",
        "caption",
        "date",
        "href",
        "id",
        "result",
      ]);
    }
  });
});
