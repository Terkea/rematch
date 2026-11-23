import { games, weaknessChangeNotes } from "@ci/db/generated";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { within } from "@testing-library/react";
import { eq, sql } from "drizzle-orm";
import { JSDOM } from "jsdom";
import type { ReactElement } from "react";
import { renderToString } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WEAKNESS_GLOSSARY } from "../../src/server/profile/glossary";
import { resetCoachingTables, seedNote } from "../helpers/coaching";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// The weaknesses pages (M5b Task 7), rendered from seeded synthetic rows through T1's read model.
// Only the session and the router are stand-ins: every read is the real one, scoped to the user.
// The file runs in node (Postgres' driver needs node's Buffer), so pages are rendered to HTML on
// the server, as Next renders them, and queried in a JSDOM document.

const db = webTestDb();
let user = { id: "", email: "x@example.invalid" };
const requireUser = vi.fn(async () => user);
const chatLive = vi.fn(() => true);
vi.mock("../../src/lib/coach-chat", () => ({ coachChatLive: () => chatLive() }));
vi.mock("../../src/server/events/request-cursor", () => ({ requestEventCursor: async () => 0 }));
vi.mock("../../src/server/auth/session", () => ({ requireUser }));
vi.mock("../../src/server/db", () => ({ getDb: () => db }));
vi.mock("next/navigation", async (orig) => ({
  ...(await orig<object>()),
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn(), prefetch: vi.fn() }),
}));

const ListPage = (await import("../../src/app/(app)/app/weaknesses/page.js")).default;
const { weeklyRates, ROW_WEEKS } = await import("../../src/app/(app)/app/weaknesses/queries.js");
const { resolveCoachScope } = await import("../../src/server/profile/index.js");
const DetailPage = (await import("../../src/app/(app)/app/weaknesses/[seriesKey]/page.js")).default;

let accountId: string;
const at = (iso: string) => new Date(`${iso}T12:00:00Z`);

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  user = { ...user, id: (await createTestSession(db)).user.id };
  accountId = (await seedAccount(db, { userId: user.id, username: "Hero" })).accountId;
});

let screen: ReturnType<typeof within>;

function show(tree: unknown) {
  const client = new QueryClient();
  const html = renderToString(
    <QueryClientProvider client={client}>{tree as ReactElement}</QueryClientProvider>,
  );
  const container = new JSDOM(`<!doctype html><body>${html}</body>`).window.document.body;
  screen = within(container);
  return { container, unmount: () => {} };
}

async function list(params: Record<string, string> = {}) {
  return show(await ListPage({ searchParams: Promise.resolve(params) }));
}

async function detail(seriesKey: string, params: Record<string, string> = {}) {
  return show(
    await DetailPage({
      params: Promise.resolve({ seriesKey }),
      searchParams: Promise.resolve(params),
    }),
  );
}

function noBadText(container: HTMLElement) {
  expect(container.textContent ?? "").not.toMatch(/NaN|undefined|Infinity|null/);
}

describe("weaknesses by colour (T3, ADR 0082)", () => {
  const none = { bandMedian: null, bandP40: null, excess: null };

  it("Both lists each colour's rate with its n under the row", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3, mean: 0.05 },
        { seriesKey: "W1", side: "white", priority: 3, mean: 0.061 },
        { seriesKey: "W1", side: "black", priority: 1, mean: 0.032 },
      ],
    });
    const { container } = await list();
    const sides = container.querySelector("[data-part='sides']");
    expect(sides?.textContent).toMatch(/White 6\.1% of [\d,]+ chances/);
    expect(sides?.textContent).toMatch(/Black 3\.2% of [\d,]+ chances/);
    noBadText(container);
  });

  it("one colour reads its own rows, shows no state, and never borrows Both's benchmark", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3, mean: 0.05 },
        { seriesKey: "W1", side: "white", state: "active", priority: 3, mean: 0.061 },
        { seriesKey: "W5", side: "white", state: "active", priority: 1, ...none },
      ],
    });
    const { container } = await list({ color: "white" });
    const text = container.textContent ?? "";
    expect(screen.queryByRole("region", { name: "Costing you games" })).toBeNull();
    expect(screen.getByRole("region", { name: "In your White games" }).textContent).toMatch(
      /6\.1%/,
    );
    const uncompared = screen.getByRole("region", { name: "No colour benchmark" });
    expect(uncompared.textContent).toMatch(/No colour benchmark/);
    expect(text).not.toMatch(/Not compared yet/);
    const link = screen.getAllByRole("link", { name: /Leaving pieces hanging/ })[0];
    expect(link?.getAttribute("href")).toMatch(/color=white/);
    noBadText(container);
  });
});

// A4: the detail has the shared bar (colour, dates), a White and Black block in Both mode, and
// every link back keeps the filters.
describe("weakness detail filters (A4)", () => {
  it("Both shows the filter bar and each colour's rate, linked to its own view", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3, mean: 0.05 },
        { seriesKey: "W1", side: "white", priority: 3, mean: 0.061 },
        { seriesKey: "W1", side: "black", priority: 1, mean: 0.032 },
      ],
    });
    const { container } = await detail("W1", { from: "2026-09-01", to: "2026-11-01" });
    expect(screen.getByRole("region", { name: "Filters" })).toBeTruthy();
    const block = screen.getByRole("region", { name: "By colour" });
    expect(block.textContent).toMatch(/White.*6\.1%/);
    expect(block.textContent).toMatch(/Black.*3\.2%/);
    const white = within(block).getByRole("link", { name: /White games only/ });
    const href = new URL(white.getAttribute("href") ?? "", "http://x");
    expect(href.pathname).toBe("/app/weaknesses/W1");
    expect(href.searchParams.get("color")).toBe("white");
    expect(href.searchParams.get("from")).toBe("2026-09-01");
    const back = screen.getByRole("link", { name: "All weaknesses" });
    const backUrl = new URL(back.getAttribute("href") ?? "", "http://x");
    expect(backUrl.searchParams.get("from")).toBe("2026-09-01");
    expect(backUrl.searchParams.get("to")).toBe("2026-11-01");
    noBadText(container);
  });

  it("one colour keeps color on the back link and shows no colour block", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3, mean: 0.05 },
        { seriesKey: "W1", side: "black", priority: 1, mean: 0.032 },
      ],
    });
    await detail("W1", { color: "black" });
    expect(screen.queryByRole("region", { name: "By colour" })).toBeNull();
    const back = screen.getByRole("link", { name: "All weaknesses" });
    expect(back.getAttribute("href")).toMatch(/color=black/);
  });

  it("Both with no colour rows shows no empty colour block", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [{ seriesKey: "W1", state: "active", priority: 3, mean: 0.05 }],
    });
    await detail("W1");
    expect(screen.queryByRole("region", { name: "By colour" })).toBeNull();
  });

  it("list rows carry the dates into the detail", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [{ seriesKey: "W1", state: "active", priority: 3, mean: 0.05 }],
    });
    await list({ from: "2026-09-01" });
    const row = screen.getAllByRole("link", { name: /Leaving pieces hanging/ })[0];
    expect(row?.getAttribute("href")).toMatch(/from=2026-09-01/);
  });

  it("list rows carry the colour and both dates into the detail", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [{ seriesKey: "W1", side: "white", state: "active", priority: 3, mean: 0.05 }],
    });
    await list({ color: "white", from: "2026-09-01", to: "2026-10-31" });
    const row = screen.getAllByRole("link", { name: /Leaving pieces hanging/ })[0];
    const q = new URL(row?.getAttribute("href") ?? "", "http://x").searchParams;
    expect([q.get("color"), q.get("from"), q.get("to")]).toEqual([
      "white",
      "2026-09-01",
      "2026-10-31",
    ]);
  });
});

describe("weaknesses list", () => {
  it("says what the dates change and what stays on the estimate (audit A6)", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [{ seriesKey: "W1", state: "active", priority: 3 }],
    });
    const { container } = await list({ from: "2026-09-01", to: "2026-10-31" });
    expect(container.querySelector("[data-slot='dates-note']")?.textContent).toBe(
      "1 Sep to 31 Oct 2026: these dates trim the weekly lines only. Each rate, its range, its state and the comparison stay on your games in the last 6 months.",
    );
    const { container: plain } = await list();
    expect(plain.querySelector("[data-slot='dates-note']")).toBeNull();
  });

  it("groups by state", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3 },
        { seriesKey: "W5", state: "emerging", priority: 5 },
        { seriesKey: "W2", state: "improving", priority: 2 },
        { seriesKey: "W4.missed", state: "resolved", priority: 1 },
        { seriesKey: "W6", state: "not_an_issue", priority: 0 },
      ],
    });
    const { container } = await list();
    const group = (name: string) =>
      within(screen.getByRole("region", { name }))
        .getAllByRole("link")
        .map((a) => a.textContent);
    const costing = group("Costing you games");
    expect(costing).toHaveLength(2);
    // By priority: W5 (5) before W1 (3).
    expect(costing[0]).toMatch(/Not winning won games/);
    expect(costing[1]).toMatch(/Leaving pieces hanging/);
    expect(group("Getting better")).toHaveLength(1);
    expect(group("Under control")).toHaveLength(2);
    const w1 = screen.getByRole("link", { name: /Leaving pieces hanging/ });
    expect(w1.getAttribute("href")).toMatch(/^\/app\/weaknesses\/W1\?/);
    expect(screen.getByText(/Based on 3 analysed rapid games/)).toBeTruthy();
    noBadText(container);
  });

  // ADR 0101 (#12): a costing state whose comparison clears 0.7 but whose evidence is below the
  // ranking floor (priority 0) is not ranked, so it never sits under "Costing you games".
  it("files a costing state below the ranking floor under worth watching", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        {
          seriesKey: "W9b",
          state: "active",
          mean: 0.0125,
          ciLow: 0.0075,
          ciHigh: 0.018,
          bandMedian: 0.008,
          bandP40: 0.0076,
          confidence: 0.87,
          nUnweighted: 846,
          kUnweighted: 11,
          priority: 0,
        },
      ],
    });
    const { container } = await list();
    expect(screen.queryByRole("region", { name: "Costing you games" })).toBeNull();
    const watching = screen.getByRole("region", { name: "Worth watching" });
    expect(within(watching).getAllByRole("link")).toHaveLength(1);
    expect(watching.textContent).toMatch(/not enough evidence yet to rank/);
    noBadText(container);
  });

  // ADR 0084 (bug hunt N10): the owner's rapid rows. "Allowing a mate" is active at confidence 0.81
  // with the peers' rate inside its range; "Early blunders" stays active (the timeline's longer
  // segment) at 0.547. Only a row the comparison backs (>= 0.7) is "Costing you games".
  it("files a costing state the comparison does not back under worth watching", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        {
          seriesKey: "W4.allowed",
          state: "active",
          mean: 0.00522,
          ciLow: 0.00293,
          ciHigh: 0.00782,
          bandMedian: 0.003498,
          bandP40: 0.00296,
          confidence: 0.81,
          nUnweighted: 1599,
          kUnweighted: 8,
          priority: 0.0092,
        },
        {
          seriesKey: "W7.traps",
          state: "active",
          mean: 0.4443,
          ciLow: 0.3819,
          ciHigh: 0.5071,
          bandMedian: 0.4381,
          bandP40: 0.4,
          confidence: 0.547,
          nUnweighted: 79,
          kUnweighted: 37,
          priority: 0.001,
        },
      ],
    });
    const { container } = await list();
    const names = (name: string) =>
      within(screen.getByRole("region", { name }))
        .getAllByRole("link")
        .map((a) => a.textContent);
    expect(names("Costing you games")).toEqual(["Allowing a mate"]);
    expect(screen.getByRole("region", { name: "Costing you games" }).textContent).toMatch(
      /inside your likely range, though yours is probably higher \(81% likely\)/,
    );
    const watching = screen.getByRole("region", { name: "Worth watching" });
    expect(within(watching).getAllByRole("link")).toHaveLength(1);
    expect(watching.textContent).toMatch(/not clearly more often/);
    noBadText(container);

    const allowed = (await detail("W4.allowed")).container.textContent ?? "";
    expect(allowed).toMatch(
      /inside your likely range, though yours is probably higher \(81% likely\)/,
    );
    expect(allowed).toMatch(/Probably more often than/);
    expect(allowed).not.toMatch(/Not clearly different/);
    const traps = (await detail("W7.traps")).container.textContent ?? "";
    expect(traps).toMatch(/Active comes from your game-by-game record/);
  });

  // Bug hunt I1: with no rating-band benchmark every estimate is stored `not_an_issue` with a
  // null excess. The page must not file them "Under control": they are not compared yet.
  it("uncompared weaknesses are not compared yet, never under control", async () => {
    const none = { state: "not_an_issue" as const, bandMedian: null, bandP40: null, excess: null };
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W5", ...none, mean: 0.325, ciLow: 0.3, ciHigh: 0.35, priority: 0 },
        { seriesKey: "W1", ...none, priority: 0 },
      ],
    });
    const { container } = await list();
    const text = container.textContent ?? "";
    expect(screen.queryByRole("region", { name: "Under control" })).toBeNull();
    expect(text).not.toMatch(/not an issue/i);
    const region = screen.getByRole("region", { name: "Not compared yet" });
    expect(region.textContent).toMatch(/no benchmark for this account yet/);
    expect(within(region).getAllByRole("link")).toHaveLength(2);
    // Each still shows its own rate, range and n, over its window.
    expect(region.textContent).toMatch(/32\.5%/);
    expect(region.textContent).toMatch(/80% range 30\.0% to 35\.0%/);
    expect(region.textContent).toMatch(/40 times in 2,000 chances in the last 6 months/);
    noBadText(container);
  });

  it("early reads get their own group, never a confident one", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W1", state: "active", priority: 3 },
        { seriesKey: "W2", state: "emerging", priority: 5, nUnweighted: 340 },
        { seriesKey: "endgame:rook", state: "improving", priority: 2 },
      ],
    });
    await list();
    const names = (name: string) =>
      within(screen.getByRole("region", { name }))
        .getAllByRole("link")
        .map((a) => a.textContent);
    expect(names("Costing you games")).toEqual(["Leaving pieces hanging"]);
    expect(screen.queryByRole("region", { name: "Getting better" })).toBeNull();
    expect(names("Early reads")).toEqual(["Missing your opponent's threat", "Rook endgames"]);
  });

  it("row sparklines leave thin and missing weeks as gaps", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      points: [
        { seriesKey: "W1", playedAt: at("2026-10-05"), events: 3, opportunities: 60 },
        // A thin week: 1 of 2 would draw a 50% spike.
        { seriesKey: "W1", playedAt: at("2026-10-12"), events: 1, opportunities: 2 },
        // 2026-10-19 and 2026-10-26: no points at all.
        { seriesKey: "W1", playedAt: at("2026-11-02"), events: 2, opportunities: 40 },
      ],
    });
    const scope = await resolveCoachScope(db, user.id, {});
    if (!scope) throw new Error("no scope");
    const trend = (await weeklyRates(db, user.id, scope)).get("W1") ?? [];
    expect(trend).toHaveLength(ROW_WEEKS);
    expect(trend.slice(-5)).toEqual([0.05, null, null, null, 0.05]);
    expect(trend.slice(0, -5).every((v) => v === null)).toBe(true);
  });

  it("empty list says when weaknesses appear", async () => {
    await seedProfile(db, accountId, { timeClass: "rapid", games: 2 });
    const { container } = await list();
    expect(
      screen.getByText("No weaknesses measured yet. They appear after about 20 analysed games."),
    ).toBeTruthy();
    noBadText(container);
  });

  it("empty list without any analysed games says the same", async () => {
    const { container } = await list();
    expect(screen.getByText(/No weaknesses measured yet/)).toBeTruthy();
    noBadText(container);
  });

  it("difficulty line counts pending_maia and hides at zero", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      difficulty: ["ok", "pending_maia", "pending_maia", "not_applicable"],
    });
    await list();
    expect(
      screen.getByText("Difficulty adjustment is waiting for Maia on 2 of 4 positions."),
    ).toBeTruthy();

    await resetGameTables(db);
    accountId = (await seedAccount(db, { userId: user.id, username: "Hero" })).accountId;
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      difficulty: ["ok", "ok"],
    });
    await list();
    expect(screen.queryByText(/waiting for Maia/)).toBeNull();
  });
});

describe("weakness detail", () => {
  it("unknown series key is not found", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    await expect(
      DetailPage({
        params: Promise.resolve({ seriesKey: "W99" }),
        searchParams: Promise.resolve({}),
      }),
    ).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    await expect(
      DetailPage({
        params: Promise.resolve({ seriesKey: "%E0%A4%A" }),
        searchParams: Promise.resolve({}),
      }),
    ).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
  });

  it("another user's weakness is not found", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    user = { ...user, id: (await createTestSession(db)).user.id };
    await expect(
      DetailPage({
        params: Promise.resolve({ seriesKey: "W1" }),
        searchParams: Promise.resolve({}),
      }),
    ).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
  });

  it("detail shows stored interval and counts", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        {
          seriesKey: "W1",
          mean: 0.0421,
          ciLow: 0.0313,
          ciHigh: 0.0566,
          kUnweighted: 38,
          nUnweighted: 1902,
          bandMedian: 0.021,
          state: "active",
        },
      ],
      points: [
        { seriesKey: "W1", playedAt: at("2026-11-03"), events: 3, opportunities: 60 },
        { seriesKey: "W1", playedAt: at("2026-11-10"), events: 1, opportunities: 12 },
      ],
    });
    const { container } = await detail("W1");
    expect(screen.getByRole("heading", { level: 1, name: "Leaving pieces hanging" })).toBeTruthy();
    expect(screen.getAllByText("4.2%").length).toBeGreaterThan(0);
    // The stored interval, one decimal each, never widened.
    expect(screen.getAllByText(/80% range 3\.1% to 5\.7%/).length).toBeGreaterThan(0);
    expect(screen.getByText(/38 times in 1,902 chances/)).toBeTruthy();
    expect(screen.getByText(/Band median 2\.1%/)).toBeTruthy();
    // The weekly series counts chances.
    expect(container.textContent).toMatch(/60 chances/);
    noBadText(container);
  });

  it("detail reads as a story: what it is, how often, the trend, examples, what to do", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        {
          seriesKey: "W1",
          mean: 0.0421,
          ciLow: 0.0313,
          ciHigh: 0.0566,
          kUnweighted: 38,
          nUnweighted: 1902,
          bandMedian: 0.021,
          state: "active",
          pImp: 0.42,
        },
      ],
    });
    const { container } = await detail("W1");
    const headings = screen
      .getAllByRole("heading", { level: 2 })
      .map((h: HTMLElement) => h.textContent);
    expect(headings.slice(0, 6)).toEqual([
      "What this is",
      "How often it happens to you",
      "Better or worse",
      "Examples from your games",
      "What to do",
      "Details",
    ]);
    const text = container.textContent ?? "";
    expect(text).toContain(WEAKNESS_GLOSSARY.W1?.meaning);
    expect(text).toContain(WEAKNESS_GLOSSARY.W1?.measured);
    expect(text).toContain(WEAKNESS_GLOSSARY.W1?.tip);
    expect(text).toContain(
      "In the last 6 months this happened in 38 of your 1,902 chances (about 1 in 50).",
    );
    // The story leads with the rate at the KPI scale (critic 5).
    expect(container.querySelector("[data-part='story-rate']")?.textContent).toBe("4.2%");
    expect(text).toMatch(/of your chances, 80% range 3\.1% to 5\.7%/);
    expect(text).toContain("More often than typical players in your rating band (2.1%)");
    expect(text).toContain(
      "No clear change yet. There is a 42% chance your rate is lower than before its last change",
    );
    noBadText(container);
  });

  it("a peer-compared weakness names the pooled rate and its chances (ADR 0081)", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        {
          seriesKey: "W4.missed",
          mean: 0.2,
          ciLow: 0.15,
          ciHigh: 0.26,
          kUnweighted: 6,
          nUnweighted: 30,
          bandMedian: 0.0957,
          state: "active",
        },
      ],
    });
    await db.execute(sql`
      insert into account_benchmark (account_id, time_class, source, center_rating, rating_low,
        rating_high, n_players, n_games)
      values (${accountId}, 'rapid', 'peers', 817, 667, 967, 73, 100)`);
    await db.execute(sql`
      insert into account_benchmark_stat (account_id, time_class, metric, median, p25, p40, p75,
        n_players, n_games, low_sample, n_chances)
      values (${accountId}, 'rapid', 'weakness:W4.missed', 0.0957, 0.07, 0.08, 0.12, 25, 26,
        false, 65)`);
    const { container } = await detail("W4.missed");
    expect(container.textContent).toContain(
      "Players you've faced, rated about 667–967: 9.6% (pooled over 65 of their chances)",
    );
    expect(container.textContent).not.toMatch(/Band median/);
    expect(container.textContent).toContain("Only their games against you count");
    // Critic 8: the story's comparison is one sentence; it names the players, no repeat line.
    const often = screen.getByRole("region", { name: "How often it happens to you" });
    expect(often.querySelector("[data-benchmark='peers']")).toBeNull();
    expect(often.textContent).toMatch(
      /the 73 players you've faced on chess\.com, rated about 667–967/i,
    );
    expect(often.textContent?.match(/players you've faced/gi)).toHaveLength(1);
    noBadText(container);
  });

  // Holistic F1: with a peer benchmark, an uncompared row says why: left out by design, or too
  // few of the opponents' chances.
  it("an uncompared row says why it is not compared", async () => {
    const none = { state: "not_an_issue" as const, bandMedian: null, bandP40: null, excess: null };
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 3,
      estimates: [
        { seriesKey: "W4.missed", mean: 0.2, ciLow: 0.15, ciHigh: 0.26, bandMedian: 0.0957 },
        { seriesKey: "W2", ...none, priority: 0 },
        { seriesKey: "W9a", ...none, priority: 0 },
      ],
    });
    await db.execute(sql`
      insert into account_benchmark (account_id, time_class, source, center_rating, rating_low,
        rating_high, n_players, n_games)
      values (${accountId}, 'rapid', 'peers', 817, 667, 967, 73, 100)`);
    await db.execute(sql`
      insert into account_benchmark_stat (account_id, time_class, metric, median, p25, p40, p75,
        n_players, n_games, low_sample, n_chances)
      values (${accountId}, 'rapid', 'weakness:W4.missed', 0.0957, 0.07, 0.08, 0.12, 25, 26,
        false, 65)`);
    const { container } = await list();
    const region = screen.getByRole("region", { name: "Not compared yet" });
    const rows = within(region).getAllByRole("listitem");
    const w2 = rows.find((r) => r.textContent?.includes(WEAKNESS_GLOSSARY.W2?.meaning ?? "?"));
    expect(w2?.textContent).toContain(
      "Not compared: this can't be measured fairly from your opponents' moves in games against you",
    );
    const w9 = rows.find((r) => !r.textContent?.includes(WEAKNESS_GLOSSARY.W2?.meaning ?? "?"));
    expect(w9?.textContent).toContain("Not compared yet: too few of their chances");
    expect(container.textContent).not.toMatch(/at your level/);
    noBadText(container);
  });

  it("an uncompared weakness says so, with its own rate and window", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        {
          seriesKey: "W5",
          state: "not_an_issue",
          bandMedian: null,
          bandP40: null,
          excess: null,
          mean: 0.325,
          ciLow: 0.3,
          ciHigh: 0.35,
          kUnweighted: 21,
          nUnweighted: 534,
        },
      ],
      history: [
        {
          seriesKey: "W5",
          kind: "transition",
          toState: "not_an_issue",
          atPlayedAt: at("2026-09-01"),
        },
      ],
    });
    const { container } = await detail("W5");
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/not an issue/i);
    expect(container.querySelector("[data-state]")?.textContent).toBe("Not compared yet");
    expect(text).toMatch(/Not compared yet: no benchmark for this account yet/);
    expect(text).toMatch(/21 times in 534 chances in the last 6 months/);
    expect(text).toMatch(/Now not compared yet/);
    noBadText(container);
  });

  it("an early read looks provisional and says what it needs", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [
        { seriesKey: "W1", nUnweighted: 120, kUnweighted: 6, bandMedian: null, state: "emerging" },
      ],
    });
    const { container } = await detail("W1");
    expect(screen.getByText("Early read: 880 more chances needed")).toBeTruthy();
    expect(screen.getByText("Not compared yet: no benchmark for this account yet")).toBeTruthy();
    expect(container.textContent).not.toMatch(/Emerging/);
    noBadText(container);
  });

  it("onset only from an onset row", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      history: [
        { seriesKey: "W1", kind: "transition", toState: "active", atPlayedAt: at("2026-10-20") },
      ],
    });
    const first = await detail("W1");
    expect(first.container.textContent).not.toMatch(/Started around/);
    noBadText(first.container);
    first.unmount();

    await seedProfile(db, accountId, {
      timeClass: "rapid",
      history: [
        {
          seriesKey: "W1",
          kind: "onset",
          toState: "active",
          onsetAt: at("2026-10-14"),
          onsetLow: at("2026-10-02"),
          onsetHigh: at("2026-10-20"),
        },
      ],
    });
    const second = await detail("W1");
    // Said in the story's trend step and again in the details' chart panel.
    expect(
      screen.getAllByText("Started around 14 Oct 2026, between 2 Oct and 20 Oct").length,
    ).toBeGreaterThan(0);
    noBadText(second.container);
  });

  it("no report means no exemplars, with the honest line", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    const { container } = await detail("W1");
    expect(
      screen.getByText(
        "No examples from your games yet: none of your analysed games in the last 6 months has a stored case of it.",
      ),
    ).toBeTruthy();
    expect(screen.queryByRole("grid")).toBeNull();
    noBadText(container);
  });

  it("exemplars name their report and link to the review ply", async () => {
    const { gameId } = await seedSyntheticGame(db, accountId, {
      playedAt: "2026-11-01T10:00:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const [g] = await db
      .select({ platformGameId: games.platformGameId })
      .from(games)
      .where(eq(games.id, gameId));
    const exemplar = {
      gameId: g?.platformGameId,
      ply: 7,
      playedAt: "2026-11-01T10:00:00.000Z",
      fen: "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
      played: { san: "Nxe5", uci: "f3e5" },
      best: { san: "Nxd4", uci: "f3d4" },
      bestPv: ["f3d4"],
      refutationPv: ["d8g5"],
      tags: ["hanging_piece"],
      evalBefore: { cp: 20, mate: null },
      evalAfter: { cp: -150, mate: null },
      winBefore: 52,
      winAfter: 36,
    };
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      input: {
        weaknesses: [
          { id: "W1", variant: null, exemplars: Array.from({ length: 7 }, () => exemplar) },
        ],
      },
    });
    const { container } = await detail("W1");
    // One example per game, at most three on the page.
    // Each example is a position explorer: it expands, and links to the review at that ply.
    const links = screen.getAllByRole("link", { name: "Open the game at 4. Nxe5" });
    expect(
      screen.getAllByRole("button", { name: "Expand the position before 4. Nxe5" }),
    ).toHaveLength(1);
    expect(links).toHaveLength(1);
    expect(container.textContent).toContain(
      "Your win chance went from 52% to 36%, a drop of 16 win% points.",
    );
    expect(links[0]?.getAttribute("href")).toBe(`/app/games/${gameId}?ply=7`);
    noBadText(container);
  });

  it("no ask link while the chat pages are not live", async () => {
    chatLive.mockReturnValue(false);
    try {
      await seedProfile(db, accountId, {
        timeClass: "rapid",
        games: 1,
        estimates: [{ seriesKey: "W3.shallow" }],
      });
      await detail("W3.shallow");
      expect(screen.queryByRole("link", { name: "Ask your coach about this" })).toBeNull();
    } finally {
      chatLive.mockReturnValue(true);
    }
  });

  it("ask link carries the weakness anchor", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W3.shallow" }, { seriesKey: "endgame:rook" }],
    });
    await detail("W3.shallow");
    const ask = screen.getByRole("link", { name: "Ask your coach about this" });
    const url = new URL(ask.getAttribute("href") ?? "", "http://x");
    expect(url.pathname).toBe("/app/coach/new");
    const anchor = url.searchParams.get("anchor") ?? "";
    const json = JSON.parse(atob(anchor.replace(/-/g, "+").replace(/_/g, "/")));
    expect(json).toEqual({ kind: "weakness", weaknessId: "W3" });
    expect(url.searchParams.get("tc")).toBe("rapid");

    await detail("endgame:rook");
    const plain = screen.getByRole("link", { name: "Ask your coach about this" });
    expect(new URL(plain.getAttribute("href") ?? "", "http://x").searchParams.has("anchor")).toBe(
      false,
    );
  });

  it("a change note from another time class is not shown", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    const { id } = await seedNote(db, accountId, {
      changeKey: "W1@blitz-game",
      seriesKey: "W1",
      onsetDate: "2026-10-02",
    });
    await db
      .update(weaknessChangeNotes)
      .set({ input: { timeClass: "blitz" } })
      .where(eq(weaknessChangeNotes.id, id));
    const { container } = await detail("W1");
    expect(container.textContent).not.toMatch(/This rose after you switched/);
    expect(container.querySelector("[data-ai-disclosure]")).toBeNull();
    noBadText(container);
  });

  it("an exemplar from another account's game has no review link", async () => {
    const other = await seedAccount(db, { username: "Stranger" });
    const { gameId } = await seedSyntheticGame(db, other.accountId, {
      playedAt: "2026-11-01T10:00:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    const [g] = await db
      .select({ platformGameId: games.platformGameId })
      .from(games)
      .where(eq(games.id, gameId));
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
      input: {
        weaknesses: [
          {
            id: "W1",
            variant: null,
            exemplars: [
              {
                gameId: g?.platformGameId,
                ply: 7,
                playedAt: "2026-11-01T10:00:00.000Z",
                fen: "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
                played: { san: "Nxe5", uci: "f3e5" },
                best: { san: "Nxd4", uci: "f3d4" },
                bestPv: ["f3d4"],
                refutationPv: ["d8g5"],
                tags: [],
                evalBefore: { cp: 20, mate: null },
                evalAfter: { cp: -150, mate: null },
                winBefore: 52,
                winAfter: 36,
              },
            ],
          },
        ],
      },
    });
    const { container } = await detail("W1");
    expect(screen.queryByRole("link", { name: /Open the game at/ })).toBeNull();
    expect(screen.getByText("This game is not among your imported games.")).toBeTruthy();
    expect(container.innerHTML).not.toContain(gameId);
  });

  it("shows the latest ready change note with its disclosure", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 1,
      estimates: [{ seriesKey: "W1" }],
    });
    await seedNote(db, accountId, { changeKey: "c1", seriesKey: "W1", onsetDate: "2026-10-02" });
    await seedNote(db, accountId, {
      changeKey: "c2",
      seriesKey: "W1",
      onsetDate: "2026-11-02",
      status: "pending",
    });
    // The change job stores its time class in `input` (Rust `WeaknessChangeInput`).
    await db.update(weaknessChangeNotes).set({ input: { timeClass: "rapid" } });
    const { container } = await detail("W1");
    expect(screen.getByText(/This rose after you switched to a new opening/)).toBeTruthy();
    expect(container.querySelector("[data-ai-disclosure]")).not.toBeNull();
    noBadText(container);
  });
});
