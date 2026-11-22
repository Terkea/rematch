import "@testing-library/jest-dom/vitest";
import { createRequire } from "node:module";
import { reportSections, reports, weaknessChangeNotes } from "@ci/db/generated";
import { within } from "@testing-library/react";
import { eq, sql } from "drizzle-orm";
import { renderToString } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadReport, withoutRawText } from "../../src/app/(app)/app/report/load.js";
import type { ReportPageData } from "../../src/app/(app)/app/report/report-view.js";
import {
  hasSeen,
  markSeen,
  revealScript,
  SEEN_KEY,
} from "../../src/app/(app)/app/report/reveal.js";
import type { ReportRunRow } from "../../src/server/coaching/types.js";
import { payloadFixture, resetCoachingTables, seedNote, seedReport } from "../helpers/coaching.js";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games.js";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session.js";

// The report page's view rendered from payloads seeded with Task 2's helper (R3's synthetic
// `backend/fixtures/ai/payload` snapshots, never owner data), one row per `reportState` case.
// The view is rendered to HTML as the server sends it and read back through a JSDOM document,
// so the database helpers run in Node (the jsdom environment breaks their Buffer and file URLs).

const chatLive = vi.fn(() => true);
vi.mock("../../src/lib/coach-chat", () => ({ coachChatLive: () => chatLive() }));
vi.mock("../../src/server/events/request-cursor", () => ({ requestEventCursor: async () => 0 }));
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
}));

const { ReportView } = await import("../../src/app/(app)/app/report/report-view.js");
const { whenText } = await import("../../src/app/(app)/app/report/rebuild-button.js");

const db = webTestDb();
const NOW = new Date("2026-11-22T12:00:00.000Z");
const CREATED = new Date("2026-11-22T08:00:00.000Z");
const RETRY = new Date("2026-11-22T14:05:00.000Z");
let userId: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  userId = (await createTestSession(db)).user.id;
  ({ accountId } = await seedAccount(db, { userId, username: "synthetic-report" }));
});

type Dom = { window: { document: Document; close(): void } };
// jsdom ships no types; only its constructor is used.
const { JSDOM } = createRequire(import.meta.url)("jsdom") as {
  JSDOM: new (html: string) => Dom;
};

/** The view's server HTML as a document body. */
function render(ui: React.ReactElement) {
  const dom = new JSDOM(`<!doctype html><body>${renderToString(ui)}</body>`);
  const container = dom.window.document.body;
  return { container, unmount: () => dom.window.close() };
}

const run = (over: Partial<ReportRunRow>) => ({ run: over });

const SCOPE = () => ({
  accountId,
  timeClass: "rapid" as const,
  platform: "chesscom" as const,
  username: "synthetic-report",
  options: [
    {
      accountId,
      timeClass: "rapid" as const,
      platform: "chesscom" as const,
      username: "synthetic-report",
      analysedGames: 214,
    },
  ],
});

/** The page's own loader, as `/app/report` calls it. */
async function data(): Promise<ReportPageData> {
  return loadReport(db, userId, SCOPE(), NOW, 30);
}

/** A report whose run finished with the AI switched off: no section rows at all. */
async function computedOnly(reason: "no_api_key" | "budget") {
  const { reportId } = await seedReport(db, accountId, {
    timeClass: "rapid",
    payload: "example",
    createdAt: CREATED,
    ...run({ status: "done", aiDisabledReason: reason }),
  });
  if (!reportId) throw new Error("no report");
  await db.delete(reportSections).where(eq(reportSections.reportId, reportId));
  await db.update(reports).set({ status: "computed_only" }).where(eq(reports.id, reportId));
}

async function previous() {
  await seedReport(db, accountId, { timeClass: "rapid", payload: "example", createdAt: CREATED });
}

async function failed(lastError: string) {
  await seedReport(db, accountId, {
    timeClass: "rapid",
    createdAt: new Date(CREATED.getTime() + 60_000),
    ...run({ status: "failed", lastError, trigger: "manual", isFirstReport: false }),
  });
}

const STATES: { name: string; seed: () => Promise<void>; copy: string[] }[] = [
  {
    name: "none",
    seed: async () => {},
    copy: ["Your coaching report hasn't been built yet.", "Build my report"],
  },
  {
    name: "first run queued",
    seed: async () => {
      await seedReport(db, accountId, { timeClass: "rapid", createdAt: CREATED });
    },
    copy: ["Building your report: 0 of 5 sections written", "Your first report is being written"],
  },
  {
    name: "running with two sections ready",
    seed: async () => {
      await seedReport(db, accountId, {
        timeClass: "rapid",
        payload: "partial",
        createdAt: CREATED,
        ...run({ status: "running" }),
      });
    },
    copy: ["Building your report: 2 of 5 sections written"],
  },
  {
    name: "awaiting retry",
    seed: async () => {
      await seedReport(db, accountId, {
        timeClass: "rapid",
        createdAt: CREATED,
        ...run({ status: "awaiting_retry", nextAttemptAt: RETRY }),
      });
    },
    copy: ["Your coach is busy; retrying at 14:05 UTC"],
  },
  {
    name: "awaiting batch",
    seed: async () => {
      await seedReport(db, accountId, {
        timeClass: "rapid",
        createdAt: CREATED,
        ...run({ status: "awaiting_batch", mode: "batch" }),
      });
    },
    copy: ["Waiting for the batch, usually within an hour"],
  },
  {
    name: "complete",
    seed: previous,
    copy: ["What's costing you games", "Hanging pieces", "Solid foundations."],
  },
  {
    name: "computed only without an API key",
    seed: () => computedOnly("no_api_key"),
    copy: ["Your coach's notes are off for now. The numbers below are computed from your games."],
  },
  {
    name: "computed only on the budget",
    seed: () => computedOnly("budget"),
    copy: [
      "Your coach has reached this month's spending limit, so this report shows the computed numbers only.",
    ],
  },
  {
    name: "failed on the plan interval",
    seed: async () => {
      await previous();
      await failed("plan_interval");
    },
    copy: ["You can rebuild this report on 22 Dec 2026."],
  },
  {
    name: "failed for no input",
    seed: () => failed("no_input"),
    copy: ["There isn't enough analysed data for a report yet."],
  },
  {
    name: "failed on the budget",
    seed: async () => {
      await previous();
      await failed("budget");
    },
    copy: [
      "Your coach has reached this month's spending limit, so this report shows the computed numbers only.",
    ],
  },
  {
    name: "failed with a previous report",
    seed: async () => {
      await previous();
      await failed("reconcile_cap");
    },
    copy: ["The last rebuild failed. Your previous report is still shown."],
  },
  {
    name: "failed with no report",
    seed: () => failed("ai_error"),
    copy: ["Building your report failed. Try again."],
  },
];

describe("report page", () => {
  it.each(STATES)("renders every report state without NaN: $name", async ({ seed, copy }) => {
    await seed();
    const { container } = render(<ReportView data={await data()} />);
    const html = container.innerHTML;
    for (const text of copy) expect(container.textContent).toContain(text);
    expect(html).not.toMatch(/NaN|undefined|Infinity|null|\{\{/);
  });

  it("partial report keeps the computed part of a dropped section", async () => {
    await seedReport(db, accountId, { timeClass: "rapid", payload: "partial", createdAt: CREATED });
    const screen = within(render(<ReportView data={await data()} />).container);
    const openings = screen.getByRole("region", { name: "Openings" });
    expect(within(openings).getByText("Sicilian Defense: Najdorf")).toBeInTheDocument();
    expect(openings).toHaveTextContent(
      "Your coach's notes for this section couldn't be checked, so they're not shown",
    );
    // The two statements the validator removed inside the weaknesses section.
    expect(openings.ownerDocument.body).toHaveTextContent(
      "2 statements were removed because they couldn't be checked against your games",
    );
  });

  it("a section not written yet says the coach is writing it while the run builds", async () => {
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "partial",
      createdAt: CREATED,
      ...run({ status: "running" }),
    });
    const screen = within(render(<ReportView data={await data()} />).container);
    const plan = screen.getByRole("region", { name: "Your climb plan" });
    expect(plan).toHaveTextContent("Your coach is writing this section");
  });

  it("move segments link to the review ply of the game's own page", async () => {
    await previous();
    // The report cites the platform id; the review page is keyed by `games.id`.
    const { gameId } = await seedSyntheticGame(db, accountId, {
      playedAt: "2026-11-01T18:30:00.000Z",
      timeClass: "rapid",
      result: "1-0",
    });
    await db.execute(sql`update games set platform_game_id = 'li-w1-0' where id = ${gameId}`);
    const screen = within(render(<ReportView data={await data()} />).container);
    const links = screen.getAllByRole("link", { name: /d4/ });
    expect(links.length).toBeGreaterThan(0);
    for (const a of links) expect(a.getAttribute("href")).toBe(`/app/games/${gameId}?ply=5`);
  });

  it("a move in a game that is not the scope's is not a link", async () => {
    await previous();
    const { container } = render(<ReportView data={await data()} />);
    expect(container).toHaveTextContent("as with d4");
    expect(container.querySelector('a[href*="li-w1-0"]')).toBeNull();
    expect(container.querySelector('a[href^="/app/games/"]')).toBeNull();
  });

  it("insufficient area says need more", async () => {
    await previous();
    const d = await data();
    const payload = d.view.payload;
    if (!payload) throw new Error("no payload");
    payload.computed.areaScores = payload.computed.areaScores.map((a) =>
      a.area === "endgame" ? { ...a, score: null, sufficient: false, needMore: 7 } : a,
    );
    const { container } = render(<ReportView data={d} />);
    expect(container).toHaveTextContent(/Need 7 more games/);
  });

  it("shows the AI disclosure only when a section is ready", async () => {
    await previous();
    const ready = render(<ReportView data={await data()} />);
    expect(ready.container).toHaveTextContent("Written by AI");
    ready.unmount();

    await resetCoachingTables(db);
    await computedOnly("no_api_key");
    const off = render(<ReportView data={await data()} />);
    expect(off.container).not.toHaveTextContent("Written by AI");
    off.unmount();

    await resetCoachingTables(db);
    await seedReport(db, accountId, { timeClass: "rapid", createdAt: CREATED });
    const building = render(<ReportView data={await data()} />);
    expect(building.container).not.toHaveTextContent("Written by AI");
  });

  it("rebuild is disabled before the interval ends and shows the date", async () => {
    await previous();
    const screen = within(render(<ReportView data={await data()} />).container);
    const button = screen.getByRole("button", { name: /Rebuild available on 22 Dec 2026/ });
    expect(button).toBeDisabled();
  });

  it("weaknesses are listed by rank with a link to ask the coach", async () => {
    await previous();
    const screen = within(render(<ReportView data={await data()} />).container);
    const section = screen.getByRole("region", { name: "What's costing you games" });
    const text = section.textContent ?? "";
    expect(text.indexOf("Hanging pieces")).toBeLessThan(text.indexOf("Missed forks"));
    const ask = within(section).getAllByRole("link", { name: /Ask your coach/ });
    expect(ask[0]?.getAttribute("href")).toMatch(/^\/app\/coach\/new\?account=.*&anchor=/);
  });

  it("no ask link while the chat pages are not live", async () => {
    chatLive.mockReturnValue(false);
    try {
      await previous();
      const screen = within(render(<ReportView data={await data()} />).container);
      const section = screen.getByRole("region", { name: "What's costing you games" });
      expect(within(section).queryAllByRole("link", { name: /Ask your coach/ })).toEqual([]);
    } finally {
      chatLive.mockReturnValue(true);
    }
  });

  it("the climb plan lists drills by name with no train button", async () => {
    await previous();
    const screen = within(render(<ReportView data={await data()} />).container);
    const plan = screen.getByRole("region", { name: "Your climb plan" });
    expect(plan).toHaveTextContent("Puzzles from your own mistakes");
    expect(within(plan).queryByRole("button", { name: /train/i })).toBeNull();
  });

  it("the basis line says how many games and whether the backfill runs", async () => {
    await previous();
    await db.execute(
      sql`update reports set computed = jsonb_set(computed, '{basis,backfillComplete}', 'false')`,
    );
    const { container } = render(<ReportView data={await data()} />);
    expect(container).toHaveTextContent("Based on 214 analysed rapid games from 3,912 imported");
    expect(container).toHaveTextContent("backfill still running");
  });
});

describe("report reveal and rebuild copy", () => {
  function memory(): Storage {
    const m = new Map<string, string>();
    return {
      getItem: (k: string) => m.get(k) ?? null,
      setItem: (k: string, v: string) => void m.set(k, v),
    } as Storage;
  }

  it("a report version is seen once marked, per version", () => {
    const s = memory();
    expect(hasSeen(s, "r1:1")).toBe(false);
    markSeen(s, "r1:1");
    expect(hasSeen(s, "r1:1")).toBe(true);
    expect(hasSeen(s, "r1:2")).toBe(false);
  });

  it("blocked or garbled storage never throws and just shows the reveal again", () => {
    const blocked = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    } as unknown as Storage;
    expect(hasSeen(blocked, "r1:1")).toBe(false);
    expect(() => markSeen(blocked, "r1:1")).not.toThrow();
    const garbled = memory();
    garbled.setItem(SEEN_KEY, "{not json");
    expect(hasSeen(garbled, "r1:1")).toBe(false);
  });

  it("the inline script cannot be closed by an id", () => {
    expect(revealScript("coaching-report", "</script><b>")).not.toContain("</script>");
  });

  it("a throttle or interval on the same day names the time, else the date", () => {
    const now = new Date("2026-11-22T12:00:00.000Z");
    expect(whenText(new Date("2026-11-22T14:05:00.000Z"), now)).toBe("at 14:05 UTC");
    expect(whenText(new Date("2026-12-22T08:00:00.000Z"), now)).toBe("on 22 Dec 2026");
    expect(whenText(new Date("2026-11-23T08:00:00.000Z"), now)).toBe("on 23 Nov at 08:00 UTC");
  });
});

describe("review fix round 1", () => {
  /** A ready note for the fixture's new change, written for `timeClass`. */
  async function note(changeKey: string, timeClass: string) {
    const { id } = await seedNote(db, accountId, {
      changeKey,
      seriesKey: "time.flag_rate",
      onsetDate: "2026-10-02",
    });
    await db
      .update(weaknessChangeNotes)
      .set({ input: { timeClass } })
      .where(eq(weaknessChangeNotes.id, id));
  }

  it("a change note shows for its own change in this time class", async () => {
    await previous();
    await note("time.flag_rate@li-c2a", "rapid");
    const screen = within(render(<ReportView data={await data()} />).container);
    const section = screen.getByRole("region", { name: "This month" });
    expect(section).toHaveTextContent("This rose after you switched to a new opening.");
  });

  it("a note from another time class or about an older change is not shown", async () => {
    await previous();
    await note("time.flag_rate@li-blitz-onset", "blitz");
    await note("time.flag_rate@li-older-onset", "rapid");
    await note("time.flag_rate@li-c2a", "blitz");
    const { container } = render(<ReportView data={await data()} />);
    expect(container).not.toHaveTextContent("This rose after you switched to a new opening.");
  });

  it("a first report never claims it started on its own", async () => {
    await seedReport(db, accountId, { timeClass: "rapid", createdAt: CREATED });
    const { container } = render(<ReportView data={await data()} />);
    expect(container).toHaveTextContent("Your first report is being written.");
    expect(container).not.toHaveTextContent("on its own");
  });

  it("mindset is never plotted, even when the payload gives it a score", async () => {
    await previous();
    const d = await data();
    expect(d.view.payload?.computed.areaScores.find((a) => a.area === "mindset")?.score).toBe(66);
    const { container } = render(<ReportView data={d} />);
    expect(container).toHaveTextContent("Mindset is coaching advice, not a score");
    expect(container).not.toHaveTextContent(/Mindset\s*66/);
  });

  it("sections without coach notes still show their numbers and say why", async () => {
    await computedOnly("no_api_key");
    const screen = within(render(<ReportView data={await data()} />).container);
    const plan = screen.getByRole("region", { name: "Your climb plan" });
    expect(plan).toHaveTextContent("Estimated 17.0% of chances");
    expect(plan).toHaveTextContent("Your coach's notes are off for this report");
    const areas = screen.getByRole("region", { name: "Area notes" });
    expect(areas).toHaveTextContent("Score 48");
  });

  it("a section being written is one quiet line, not a skeleton", async () => {
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "partial",
      createdAt: CREATED,
      ...run({ status: "running" }),
    });
    const screen = within(render(<ReportView data={await data()} />).container);
    const plan = screen.getByRole("region", { name: "Your climb plan" });
    expect(plan).toHaveTextContent("Your coach is writing this section");
    expect(plan.querySelector("[data-coach-state]")).toBeNull();
  });

  it("the building bar shows the sections written, never an empty bar beside the count", async () => {
    await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "partial",
      createdAt: CREATED,
      ...run({ status: "running" }),
    });
    const { container } = render(<ReportView data={await data()} />);
    const stroke = container.querySelector('[data-part="stroke"]');
    expect(stroke?.getAttribute("data-fraction")).toBe("0.4");
  });

  it("a phone shows one example position per weakness with a way to see the rest", async () => {
    await previous();
    const screen = within(render(<ReportView data={await data()} />).container);
    expect(screen.getAllByRole("button", { name: "Show 2 more positions" }).length).toBe(2);
  });

  it("raw model text never reaches the browser", async () => {
    await previous();
    const d = await data();
    expect(JSON.stringify(d)).not.toContain("{{");
    expect(withoutRawText({ text: "{{num:x}}", segments: [] })).toEqual({ segments: [] });
  });

  it("the report loads in a fixed number of reads, whatever the number of weaknesses", async () => {
    await previous();
    const many = Array.from({ length: 40 }, (_, i) => ({
      ...(payloadWeakness() as object),
      id: `W${i + 100}`,
      rank: i + 1,
    }));
    await db.execute(
      sql`update reports set computed = jsonb_set(computed, '{weaknesses}', ${JSON.stringify(many)}::jsonb)`,
    );
    await data();
    const times: number[] = [];
    for (let i = 0; i < 5; i++) {
      const t0 = performance.now();
      await data();
      times.push(performance.now() - t0);
    }
    times.sort((a, b) => a - b);
    expect(times[2]).toBeLessThan(150);
  });
});

function payloadWeakness(): unknown {
  return payloadFixture("example").computed.weaknesses[0];
}
