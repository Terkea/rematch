import { games, weaknessChangeNotes } from "@ci/db/generated";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { within } from "@testing-library/react";
import { eq } from "drizzle-orm";
import { JSDOM } from "jsdom";
import type { ReactElement } from "react";
import { renderToString } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

describe("weaknesses list", () => {
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
    expect(costing[0]).toMatch(/Conversion failure/);
    expect(costing[1]).toMatch(/Hung material/);
    expect(group("Getting better")).toHaveLength(1);
    expect(group("Under control")).toHaveLength(2);
    const w1 = screen.getByRole("link", { name: /Hung material/ });
    expect(w1.getAttribute("href")).toMatch(/^\/app\/weaknesses\/W1\?/);
    expect(screen.getByText(/Based on 3 analysed rapid games/)).toBeTruthy();
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
    expect(names("Costing you games")).toEqual(["Hung material"]);
    expect(screen.queryByRole("region", { name: "Getting better" })).toBeNull();
    expect(names("Early reads")).toEqual(["Missed opponent threat", "Rook endgames"]);
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
    expect(screen.getByRole("heading", { level: 1, name: "Hung material" })).toBeTruthy();
    expect(screen.getAllByText("4.2%").length).toBeGreaterThan(0);
    // The stored interval, one decimal each, never widened.
    expect(screen.getAllByText(/80% range 3\.1–5\.7%/).length).toBeGreaterThan(0);
    expect(screen.getByText(/38 times in 1,902 chances/)).toBeTruthy();
    expect(screen.getByText(/Band median 2\.1%/)).toBeTruthy();
    // The weekly series counts chances.
    expect(container.textContent).toMatch(/60 chances/);
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
    expect(screen.getByText("No band reference yet")).toBeTruthy();
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
    expect(screen.getByText("Started around 14 Oct 2026, between 2 Oct and 20 Oct")).toBeTruthy();
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
      screen.getByText("Example positions appear once your first report is built."),
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
    const from = screen.getByText(
      (_: string, el: Element | null) =>
        el?.tagName === "P" &&
        /^Examples from your report of \d{1,2} \w{3} \d{4}$/.test(el.textContent ?? ""),
    );
    expect(from.querySelector("time")?.getAttribute("dateTime")).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    const links = screen.getAllByRole("link", { name: /Open 4\. Nxe5 in the review/ });
    expect(links).toHaveLength(5);
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
    expect(screen.queryByRole("link", { name: /in the review/ })).toBeNull();
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
