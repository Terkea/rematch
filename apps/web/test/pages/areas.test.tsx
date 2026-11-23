// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, screen, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReportView } from "../../src/server/coaching";
import type { ReportPayload } from "../../src/server/coaching/types";
import type {
  AreaView,
  CoachScope,
  KpiTrend,
  WeaknessDetail,
  WeaknessSummary,
} from "../../src/server/profile/types";

// R3's synthetic payload snapshot, read by path: jsdom's URL is not Node's, so the shared
// `payloadFixture` helper can't run in this environment.
const FIXTURE = join(
  dirname(fileURLToPath(import.meta.url)),
  "../../../../backend/fixtures/ai/payload/report-payload.example.json",
);
const payloadFixture = (): ReportPayload =>
  JSON.parse(readFileSync(FIXTURE, "utf8")) as ReportPayload;

// The seven skill-area pages (M5b Task 8). Every read goes through T1's read model and T2's
// report read model as the signed-in user; the page never reads a table itself. Synthetic data
// only: R3's payload fixture and hand-made rows.

const ACCOUNT = "00000000-0000-4000-8000-0000000000a1";
const SCOPE: CoachScope = {
  accountId: ACCOUNT,
  timeClass: "rapid",
  platform: "lichess",
  username: "synthetic_player",
  options: [
    {
      accountId: ACCOUNT,
      platform: "lichess",
      username: "synthetic_player",
      timeClass: "rapid",
      analysedGames: 214,
    },
  ],
};
const AT = new Date("2026-11-16T09:00:00Z");

function weakness(over: Partial<WeaknessSummary> = {}): WeaknessSummary {
  return {
    seriesKey: "W8.basics",
    contractId: "W8",
    label: "Endgame technique (basics)",
    area: "endgame",
    state: "active",
    rate: { mean: 0.21, low: 0.15, high: 0.28, level: 0.8 },
    evidence: { events: 31, chances: 148, minEvidence: 80, sufficient: true, needMore: null },
    band: { median: 0.12, p40: 0.1 },
    compared: true,
    priority: 4,
    confidence: 0.8,
    pImp: null,
    updatedAt: AT,
    onset: { at: new Date("2026-09-14T00:00:00Z"), low: null, high: null, gameId: null },
    ...over,
  };
}

function trend(over: Partial<KpiTrend> & { metric: string; label: string }): KpiTrend {
  return {
    unit: "pct",
    current: null,
    points: [],
    ...over,
  };
}

function current(metric: string, label: string, unit: string, n: number, minN: number) {
  return {
    metric,
    label,
    unit,
    higherIsBetter: false,
    value: 0.2,
    low: null,
    high: null,
    n,
    minN,
    sufficient: n >= minN,
    needMore: n >= minN ? null : minN - n,
    computedAt: AT,
  };
}

const ENDGAME_VIEW: AreaView = {
  area: "endgame",
  score: { value: 61.6, n: 46, computedAt: AT },
  // The area's last-50 rows: Rust's area n is the smallest component sample (games for a mean,
  // chances for a rate), so the page can name it.
  kpis: [
    current("accuracy.endgame", "Endgame accuracy", "pct", 46, 20),
    current("weakness:W8.basics", "Endgame technique (basics)", "rate", 148, 80),
    current("weakness:W8.advanced", "Endgame technique (advanced)", "rate", 96, 80),
  ],
  weaknesses: [
    weakness(),
    // A tactics weakness that should never reach the endgame page.
    weakness({ seriesKey: "W1", contractId: "W1", label: "Hung material", area: "calculation" }),
  ],
  timeline: [
    {
      id: "W8.basics",
      label: "Endgame technique (basics)",
      segments: [
        { state: "emerging", from: "2026-08-01", to: "2026-09-14" },
        { state: "active", from: "2026-09-14", to: null },
      ],
    },
  ],
};

const KPIS: KpiTrend[] = [
  trend({
    metric: "accuracy.endgame",
    label: "Endgame accuracy",
    unit: "pct",
    current: {
      metric: "accuracy.endgame",
      label: "Endgame accuracy",
      unit: "pct",
      higherIsBetter: true,
      value: 71.4,
      low: null,
      high: null,
      n: 46,
      minN: 20,
      sufficient: true,
      needMore: null,
      computedAt: AT,
    },
    points: [
      // The read model's keys are ISO weeks (kpiTrends keeps only `YYYY-Www` rows).
      { week: "2026-W45", value: 70.2, low: null, high: null, n: 9 },
      { week: "2026-W46", value: 72.8, low: null, high: null, n: 12 },
    ],
  }),
  trend({
    metric: "endgame_hold_rate",
    label: "Drawn endgames held",
    unit: "rate",
    current: {
      metric: "endgame_hold_rate",
      label: "Drawn endgames held",
      unit: "rate",
      higherIsBetter: true,
      value: 0.5,
      low: 0.3,
      high: 0.7,
      n: 7,
      minN: 15,
      sufficient: false,
      needMore: 8,
      computedAt: AT,
    },
    points: [{ week: "2026-W46", value: 0.5, low: null, high: null, n: 2 }],
  }),
  trend({
    metric: "tb_errors_per_endgame",
    label: "Tablebase errors per endgame",
    unit: "per_game",
  }),
];

const detail = (w: WeaknessSummary): WeaknessDetail => ({
  ...w,
  window: { days: 183, from: new Date(AT.getTime() - 183 * 86_400_000), to: AT },
  history: [],
  examples: [],
  timeline: { id: w.seriesKey, label: w.label, segments: [] },
  weekly: [
    {
      week: "2026-11-02",
      events: 3,
      chances: 14,
      rate: { value: 0.21, low: 0.1, high: 0.3, level: 0.8 },
    },
    { week: "2026-11-09", events: 0, chances: 0, rate: null },
  ],
  exemplars: null,
});

function reportView(payload: ReportPayload | null): ReportView {
  return {
    state: payload ? { kind: "ready", status: "complete", aiOff: null } : { kind: "none" },
    payload,
    createdAt: payload ? new Date("2026-11-20T08:30:00Z") : null,
    nextManualAt: null,
  };
}

/** The R3 example payload with the endgame weakness given positions and a needMore set. */
function payload(): ReportPayload {
  const p = payloadFixture();
  const tactic = p.computed.weaknesses.find((w) => w.id === "W1");
  const endgame = p.computed.weaknesses.find((w) => w.area === "endgame");
  if (!tactic || !endgame) throw new Error("fixture changed");
  // Two of W1's positions, re-labelled as an endgame weakness's, so the page has some to show.
  endgame.exemplars = tactic.exemplars.slice(0, 2);
  return p;
}

const resolveCoachScope = vi.fn();
const getArea = vi.fn();
const kpiTrends = vi.fn();
const getWeakness = vi.fn();
const getReport = vi.fn();
const getBenchmark = vi.fn(async () => null);
const requireUser = vi.fn(async () => ({ id: "u1", email: "x@example.invalid" }));

const coachStatus = vi.fn(async () => "available");
vi.mock("../../src/server/coach/status", async (orig) => ({
  ...(await orig<object>()),
  coachStatus: () => coachStatus(),
}));
const chatLive = vi.fn(() => true);
vi.mock("../../src/lib/coach-chat", () => ({ coachChatLive: () => chatLive() }));
vi.mock("../../src/server/events/request-cursor", () => ({ requestEventCursor: async () => 0 }));
vi.mock("../../src/server/auth/session", () => ({ requireUser }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));
vi.mock("../../src/server/profile", async (orig) => ({
  ...(await orig<object>()),
  resolveCoachScope,
  getArea,
  kpiTrends,
  getWeakness,
  getBenchmark,
}));
vi.mock("../../src/server/coaching", async (orig) => ({
  ...(await orig<object>()),
  getReport,
}));
vi.mock("next/navigation", async (orig) => ({
  ...(await orig<object>()),
  useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }),
}));

const Page = (await import("../../src/app/(app)/app/areas/[area]/page.js")).default;
const IndexPage = (await import("../../src/app/(app)/app/areas/page.js")).default;

const args = (area: string, search: Record<string, string> = {}) => ({
  params: Promise.resolve({ area }),
  searchParams: Promise.resolve(search),
});

async function show(area: string, search: Record<string, string> = {}) {
  const tree = (await Page(args(area, search))) as ReactElement;
  return render(tree);
}

const BAD = /NaN|undefined|Infinity|null/;

beforeEach(() => {
  for (const f of [resolveCoachScope, getArea, kpiTrends, getWeakness, getReport]) f.mockReset();
  resolveCoachScope.mockResolvedValue(SCOPE);
  getArea.mockImplementation(async (_db, _u, _s, area: string) =>
    area === "endgame"
      ? ENDGAME_VIEW
      : { area, score: null, kpis: [], weaknesses: [], timeline: [] },
  );
  kpiTrends.mockImplementation(async (_db, _u, _s, metrics: string[]) =>
    KPIS.filter((k) => metrics.includes(k.metric)),
  );
  getWeakness.mockImplementation(async (_db, _u, _s, key: string) => {
    const w = ENDGAME_VIEW.weaknesses.find((x) => x.seriesKey === key);
    return w ? detail(w) : null;
  });
  getReport.mockResolvedValue(reportView(payload()));
});

// Each page render mounts real components (up to three boards, the KPI grid and the timeline),
// which is slow on a loaded CI host, so the page tests get more than the 5 s default.
describe("skill-area pages", { timeout: 30_000 }, () => {
  it("unknown area is not found", async () => {
    await expect(Page(args("kingside"))).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    // Nothing is read for an unknown area, not even the user.
    for (const f of [requireUser, resolveCoachScope, getArea, kpiTrends, getWeakness, getReport])
      expect(f).not.toHaveBeenCalled();
  });

  it("reads through the read models as the signed-in user, in the resolved scope", async () => {
    await show("endgame", { account: ACCOUNT, tc: "rapid" });
    expect(resolveCoachScope).toHaveBeenCalledWith({}, "u1", { account: ACCOUNT, tc: "rapid" });
    // The scope carries the colour filter's stored side (Both until ADR 0082, T3).
    const scoped = { ...SCOPE, side: "both" };
    expect(getArea).toHaveBeenCalledWith({}, "u1", scoped, "endgame");
    expect(getReport).toHaveBeenCalledWith({}, "u1", { accountId: ACCOUNT, timeClass: "rapid" });
    for (const call of [...kpiTrends.mock.calls, ...getWeakness.mock.calls]) {
      expect(call[1]).toBe("u1");
      expect(call[2]).toEqual(scoped);
    }
  });

  it("shows the area score with its n", async () => {
    const { container } = await show("endgame");
    expect(screen.getByText(/Endgame 62, band median 50/)).toBeInTheDocument();
    expect(container.textContent).toMatch(/46 rapid games/);
  });

  it("mindset says it has no score", async () => {
    const { container } = await show("mindset");
    expect(screen.getByText("Mindset is coaching advice, not a score")).toBeInTheDocument();
    // The report fixture gives mindset a number; the page never shows one.
    expect(container.querySelector("[data-area-label]")).toBeNull();
    expect(container.textContent).not.toMatch(/Mindset 66/);
  });

  it("null score says not enough games and never renders 0", async () => {
    getReport.mockResolvedValue(reportView(null));
    getArea.mockResolvedValue({
      area: "time",
      score: { value: null, n: 6, computedAt: AT },
      kpis: [current("weakness:W9a", "Time trouble", "rate", 6, 30)],
      weaknesses: [],
      timeline: [],
    });
    const { container } = await show("time");
    const score = within(screen.getByRole("region", { name: "Score" }));
    expect(score.getByText("Not enough games for a score yet")).toBeInTheDocument();
    // The n is the thinnest component's, in its own unit: Time trouble counts chances.
    expect(
      score.getByText("Its thinnest measure, Time trouble, has 6 chances so far."),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-area-label]")).toBeNull();
    expect(screen.getByRole("region", { name: "Score" }).textContent).not.toMatch(/\b0\b/);
  });

  it("needMore from the report is shown", async () => {
    const p = payload();
    const row = p.computed.areaScores.find((a) => a.area === "endgame");
    if (!row) throw new Error("fixture changed");
    Object.assign(row, { score: null, sufficient: false, needMore: 7, n: 13 });
    getReport.mockResolvedValue(reportView(p));
    // Rust's needMore is the first short component's: here Endgame accuracy, 13 of 20 games.
    getArea.mockResolvedValue({
      ...ENDGAME_VIEW,
      score: null,
      kpis: [current("accuracy.endgame", "Endgame accuracy", "pct", 13, 20)],
    });
    await show("endgame");
    expect(screen.getByText("Need 7 more games for an endgame score")).toBeInTheDocument();
  });

  it("needMore is stated in its component's unit", async () => {
    const p = payload();
    const row = p.computed.areaScores.find((a) => a.area === "time");
    if (!row) throw new Error("fixture changed");
    Object.assign(row, { score: null, sufficient: false, needMore: 9, n: 21 });
    getReport.mockResolvedValue(reportView(p));
    getArea.mockResolvedValue({
      area: "time",
      score: { value: null, n: 21, computedAt: AT },
      kpis: [current("weakness:W9a", "Time trouble", "rate", 21, 30)],
      weaknesses: [],
      timeline: [],
    });
    await show("time");
    expect(screen.getByText("Need 9 more chances for a time-management score")).toBeInTheDocument();
  });

  it("a needMore that matches no measure names no unit", async () => {
    const p = payload();
    const row = p.computed.areaScores.find((a) => a.area === "time");
    if (!row) throw new Error("fixture changed");
    Object.assign(row, { score: null, sufficient: false, needMore: 9, n: 21 });
    getReport.mockResolvedValue(reportView(p));
    getArea.mockResolvedValue({
      area: "time",
      score: { value: null, n: 21, computedAt: AT },
      kpis: [],
      weaknesses: [],
      timeline: [],
    });
    await show("time");
    const region = screen.getByRole("region", { name: "Score" });
    expect(within(region).getByText("Not enough games for a score yet")).toBeInTheDocument();
    expect(region.textContent).toMatch(/Its thinnest measure needs 9 more\./);
    expect(region.textContent).not.toMatch(/9 more (games|chances)/);
  });

  it("kpis carry n and need-more", async () => {
    await show("endgame");
    const kpis = within(screen.getByRole("region", { name: "Measures" }));
    expect(kpis.getByRole("heading", { name: "Endgame accuracy" })).toBeInTheDocument();
    expect(kpis.getAllByText(/12 games/).length).toBeGreaterThan(0);
    const thin = kpis.getByText(/Drawn endgames held/).closest("li");
    expect(thin?.textContent).toMatch(/7 games so far/);
    expect(thin?.textContent).toMatch(/Need 8 more games/);
    // A thin measure shows no value: 50% from 7 games would read as settled.
    expect(thin?.textContent).not.toMatch(/50/);
    const none = kpis.getByText(/Tablebase errors per endgame/).closest("li");
    expect(none?.textContent).toMatch(/Not measured yet/);
    // Only the area's own non-weakness measures are asked for.
    const asked = kpiTrends.mock.calls[0]?.[3] as string[];
    expect(asked).toContain("accuracy.endgame");
    expect(asked.every((m) => !m.startsWith("weakness:"))).toBe(true);
    expect(asked).not.toContain("accuracy.opening");
  });

  it("dates each measure's weeks from the ISO week key (B2)", async () => {
    await show("endgame");
    const region = screen.getByRole("region", { name: "Measures" });
    expect(region.textContent).not.toMatch(/unknown date/);
    // 2026-W46 starts on Monday 9 Nov 2026.
    expect(region.textContent).toMatch(/Week of 9 Nov 2026/);
    const times = [...region.querySelectorAll("time")].map((t) => t.getAttribute("dateTime"));
    expect(times).toContain("2026-11-09");
    expect(times.every((t) => t == null || /^\d{4}-\d{2}-\d{2}$/.test(t))).toBe(true);
  });

  it("area weaknesses only", async () => {
    await show("endgame");
    const list = within(screen.getByRole("region", { name: "Weaknesses" }));
    expect(list.getByRole("link", { name: "Endgame technique (basics)" })).toHaveAttribute(
      "href",
      `/app/weaknesses/W8.basics?account=${ACCOUNT}&tc=rapid`,
    );
    expect(list.queryByText("Hung material")).toBeNull();
    expect(getWeakness).not.toHaveBeenCalledWith({}, "u1", SCOPE, "W1");
    // The timeline is the area's own.
    expect(screen.getByRole("region", { name: "Weaknesses" }).textContent).toMatch(
      /Endgame technique \(basics\)/,
    );
    // Positions come from this area's ranked weaknesses only, at most 3, dated by the report.
    const examples = screen.getByRole("region", { name: "Example positions" });
    expect(examples.textContent).toMatch(/Examples from your report of 20 Nov 2026/);
    expect(examples.querySelectorAll("[data-exemplar]").length).toBe(2);
  });

  it("an area without report positions says when they appear", async () => {
    getReport.mockResolvedValue(reportView(null));
    await show("endgame");
    expect(
      screen.getByText("Example positions appear once your first report is built."),
    ).toBeInTheDocument();
  });

  it("no ask link while the chat pages are not live", async () => {
    chatLive.mockReturnValue(false);
    try {
      await show("endgame");
      expect(screen.queryByRole("link", { name: /Ask your coach/ })).toBeNull();
    } finally {
      chatLive.mockReturnValue(true);
    }
  });

  it("shows the coach's note with its disclosure and asks about the area", async () => {
    await show("endgame");
    const note = within(screen.getByRole("region", { name: "Coach's note" }));
    expect(note.getByText("Your endgame play looks sound.")).toBeInTheDocument();
    expect(note.getByText("Written by AI")).toBeInTheDocument();
    const ask = screen.getByRole("link", { name: "Ask your coach about your endgame" });
    const href = new URL(ask.getAttribute("href") ?? "", "http://x");
    expect(href.pathname).toBe("/app/coach/new");
    expect(href.searchParams.get("account")).toBe(ACCOUNT);
    expect(href.searchParams.get("anchor")).not.toBeNull();
  });

  it("asks the coach about the colour the page is filtered to (audit A3)", async () => {
    await show("endgame", { account: ACCOUNT, tc: "rapid", color: "black" });
    const ask = screen.getByRole("link", { name: "Ask your coach about your endgame" });
    const href = new URL(ask.getAttribute("href") ?? "", "http://x");
    expect(href.searchParams.get("color")).toBe("black");
  });

  it("keeps the colour and dates on the area tabs (audit A7)", async () => {
    await show("endgame", {
      account: ACCOUNT,
      tc: "rapid",
      color: "white",
      from: "2026-09-01",
      to: "2026-10-31",
    });
    const tabs = within(screen.getByRole("navigation", { name: "Skill areas" }));
    const href = new URL(
      tabs.getByRole("link", { name: "Time" }).getAttribute("href") ?? "",
      "http://x",
    );
    expect(Object.fromEntries(href.searchParams)).toMatchObject({
      color: "white",
      from: "2026-09-01",
      to: "2026-10-31",
    });
  });

  it("says what the dates change and what stays on its window (audit A6)", async () => {
    await show("endgame", { account: ACCOUNT, tc: "rapid", from: "2026-09-01" });
    expect(
      screen.getByText(/^From 1 Sep 2026: these dates trim the weekly lines only\./),
    ).toBeInTheDocument();
  });

  it("a dropped areas section says so and shows no model text", async () => {
    const p = payload();
    p.sections.areas = null;
    getReport.mockResolvedValue(reportView(p));
    await show("endgame");
    expect(
      screen.getByText(/couldn't be checked against your games, so it isn't shown/),
    ).toBeInTheDocument();
    expect(screen.queryByText("Your endgame play looks sound.")).toBeNull();
  });

  it("links the seven areas, keeping the scope", async () => {
    await show("endgame", { account: ACCOUNT, tc: "rapid" });
    const tabs = within(screen.getByRole("navigation", { name: "Skill areas" }));
    const links = tabs.getAllByRole("link");
    expect(links.map((l) => l.textContent)).toEqual([
      "Openings",
      "Middlegame",
      "Endgame",
      "Tactics",
      "Calculation",
      "Time",
      "Mindset",
    ]);
    expect(tabs.getByRole("link", { name: "Endgame" })).toHaveAttribute("aria-current", "page");
    expect(tabs.getByRole("link", { name: "Time" })).toHaveAttribute(
      "href",
      `/app/areas/time?account=${ACCOUNT}&tc=rapid`,
    );
  });

  it("a user with no analysed games gets the empty state", async () => {
    resolveCoachScope.mockResolvedValue(null);
    await show("endgame");
    expect(screen.getByText(/once some of your games are analysed/)).toBeInTheDocument();
    expect(getArea).not.toHaveBeenCalled();
  });

  it("the score key names no range while there is no interval", async () => {
    await show("endgame");
    const score = within(screen.getByRole("region", { name: "Score" }));
    expect(score.getByText("No interval yet")).toBeInTheDocument();
    expect(score.queryByText("Likely range")).toBeNull();
    expect(score.getByText("Your score")).toBeInTheDocument();
  });

  it("the score's basis is its thinnest measure, in that measure's unit", async () => {
    const first = await show("endgame");
    const region = screen.getByRole("region", { name: "Score" });
    expect(within(region).getByText("46 rapid games")).toBeInTheDocument();
    expect(region.textContent).toMatch(/thinnest measure, Endgame accuracy/);
    // No count other than Rust's n is claimed.
    expect(region.textContent).not.toMatch(/last 50/);
    first.unmount();

    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: { value: 58, n: 96, computedAt: AT } });
    await show("endgame");
    const again = screen.getByRole("region", { name: "Score" });
    expect(within(again).getByText("96 chances")).toBeInTheDocument();
    expect(again.textContent).toMatch(/thinnest measure, Endgame technique \(advanced\)/);
  });

  it("names the players faced, not a rating band, against a peer benchmark (Q8)", async () => {
    getBenchmark.mockResolvedValueOnce({
      source: "peers",
      platform: "chesscom",
      low: 667,
      high: 967,
      players: 25,
      games: 61,
      chances: {},
    } as never);
    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: { value: 44.33, n: 46, computedAt: AT } });
    await show("endgame");
    const region = screen.getByRole("region", { name: "Score" });
    expect(region.textContent).toContain(
      "Against players you've faced on chess.com, rated about 667–967: 50 is level with them and higher is better.",
    );
    expect(region.textContent).not.toMatch(/rating band|band median/);
  });

  it("a score whose n matches no measure names no unit", async () => {
    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: { value: 58, n: 31, computedAt: AT } });
    await show("endgame");
    const region = screen.getByRole("region", { name: "Score" });
    expect(within(region).getByText("31 in its thinnest measure")).toBeInTheDocument();
    expect(region.textContent).not.toMatch(/31 (rapid )?games|31 chances/);
  });

  it("an area the profile hasn't scored says so, with no guessed count", async () => {
    getReport.mockResolvedValue(reportView(null));
    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: null });
    await show("endgame");
    const region = screen.getByRole("region", { name: "Score" });
    expect(
      within(region).getByText("Your profile hasn't scored this area yet."),
    ).toBeInTheDocument();
  });

  it("a report older than the live score gives no needMore", async () => {
    const p = payload();
    const row = p.computed.areaScores.find((a) => a.area === "endgame");
    if (!row) throw new Error("fixture changed");
    Object.assign(row, { score: null, sufficient: false, needMore: 7, n: 13 });
    getReport.mockResolvedValue({ ...reportView(p), createdAt: new Date("2026-11-01T00:00:00Z") });
    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: { value: null, n: 46, computedAt: AT } });
    await show("endgame");
    expect(screen.queryByText(/Need 7 more games/)).toBeNull();
    expect(screen.getByText("Not enough games for a score yet")).toBeInTheDocument();
  });

  it("a numbers-only note says the coach is not available only when it is not", async () => {
    const absent = payload();
    delete absent.sections.areas;
    getReport.mockResolvedValue({
      ...reportView(absent),
      state: { kind: "ready", status: "computed_only", aiOff: "no_api_key" },
    });
    coachStatus.mockResolvedValueOnce("no_ai");
    await show("endgame");
    const note = screen.getByRole("region", { name: "Coach's note" });
    expect(note.textContent).toMatch(/Coach not available right now/);
    expect(note.textContent).not.toMatch(/notes are off|next rebuild/);
  });

  it("the note's states read honestly", async () => {
    const absent = payload();
    delete absent.sections.areas;
    const cases: [ReportView, RegExp][] = [
      [
        {
          ...reportView(absent),
          state: {
            kind: "building",
            runId: "r",
            since: AT,
            retryAt: null,
            batch: false,
            ready: [],
          },
        },
        /Your coach is writing this section/,
      ],
      [
        {
          ...reportView(absent),
          state: { kind: "ready", status: "computed_only", aiOff: "budget" },
        },
        /spending limit/,
      ],
      [
        {
          ...reportView(absent),
          state: { kind: "ready", status: "computed_only", aiOff: "no_api_key" },
          nextManualAt: new Date("2026-12-20T08:30:00Z"),
        },
        // Fix D (N2): the coach is on; this report was built before it was.
        /built from your numbers only.*notes come with the next rebuild, available on 20 Dec 2026/,
      ],
      // A partial report without the section, with AI on, is not "notes off".
      [
        { ...reportView(absent), state: { kind: "ready", status: "partial", aiOff: null } },
        /didn't write a note on your endgame/,
      ],
    ];
    for (const [view, text] of cases) {
      getReport.mockResolvedValue(view);
      const { unmount } = await show("endgame");
      const note = screen.getByRole("region", { name: "Coach's note" });
      expect(note.textContent).toMatch(text);
      expect(note.textContent).not.toMatch(/looks sound/);
      unmount();
    }
    const noItem = payload();
    const areas = noItem.sections.areas?.areas;
    if (!areas) throw new Error("fixture changed");
    areas.items = areas.items.filter((i) => i.area !== "endgame");
    getReport.mockResolvedValue(reportView(noItem));
    await show("endgame");
    expect(screen.getByRole("region", { name: "Coach's note" }).textContent).toMatch(
      /didn't write a note on your endgame in this report/,
    );
  });

  it("scrolls the current tab into view and fades the edges that have more", async () => {
    // jsdom has no layout: give the tab track a 300px window on 800px of tabs, with the current
    // tab (Mindset, the last) at 600px.
    const proto = HTMLElement.prototype;
    const spies = [
      vi.spyOn(proto, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
        return this.hasAttribute("data-scroller") ? 300 : 0;
      }),
      vi.spyOn(proto, "scrollWidth", "get").mockImplementation(function (this: HTMLElement) {
        return this.hasAttribute("data-scroller") ? 800 : 0;
      }),
      vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (
        this: Element,
      ) {
        const current = this.getAttribute("aria-current") === "page";
        return { left: current ? 600 : 0, width: current ? 80 : 300 } as DOMRect;
      }),
    ];
    try {
      await show("mindset");
      const nav = screen.getByRole("navigation", { name: "Skill areas" });
      const track = nav.hasAttribute("data-scroller") ? nav : null;
      // Centred on the current tab: 600 - (300 - 80) / 2 = 490, within the 500 that can scroll.
      expect(track?.scrollLeft).toBe(490);
      expect(track?.dataset.fade).toBe("start end");
    } finally {
      for (const s of spies) s.mockRestore();
    }
  });

  it("renders no NaN, undefined, Infinity or null on any area", async () => {
    for (const area of [
      "openings",
      "middlegame",
      "endgame",
      "tactics",
      "calculation",
      "time",
      "mindset",
    ]) {
      const { container, unmount } = await show(area);
      expect(container.textContent).not.toMatch(BAD);
      unmount();
    }
    getReport.mockResolvedValue(reportView(null));
    const { container } = await show("endgame");
    expect(container.textContent).not.toMatch(BAD);
  });
});

describe("area score components", () => {
  it("mirror Rust's area_components, in order", async () => {
    const { AREA_COMPONENTS } = await import("../../src/app/(app)/app/areas/[area]/basis.js");
    const rust = readFileSync(
      join(
        dirname(fileURLToPath(import.meta.url)),
        "../../../../backend/crates/profile-core/src/kpi/area_scores.rs",
      ),
      "utf8",
    );
    // `Base(B::AccuracyOpening)` and `Weakness(W::W7Traps)` against `accuracy.opening` and
    // `weakness:W7.traps`, compared without case, dots or underscores.
    const flat = (x: string) =>
      x
        .replace(/^weakness:/, "")
        .replace(/[._]/g, "")
        .toLowerCase();
    for (const [area, metrics] of Object.entries(AREA_COMPONENTS)) {
      const body = new RegExp(
        `const ${area.toUpperCase()}: \\[MetricId; \\d+\\] = \\[([^\\]]*)\\]`,
      ).exec(rust)?.[1];
      expect(body, area).toBeDefined();
      const names = [...(body ?? "").matchAll(/(?:B|W)::(\w+)/g)].map((m) =>
        (m[1] ?? "").toLowerCase(),
      );
      expect(metrics.map(flat), area).toEqual(names);
    }
  });
});

describe("/app/areas", () => {
  it("redirects to openings, keeping the scope params", async () => {
    const err = await IndexPage({
      searchParams: Promise.resolve({ account: ACCOUNT, tc: "rapid", other: "x" }),
    }).catch((e: unknown) => e);
    const digest = String((err as { digest?: string }).digest);
    expect(digest).toContain(`/app/areas/openings?account=${ACCOUNT}&tc=rapid;`);
    expect(digest).not.toContain("other");
  });

  it("keeps the colour and dates too (audit A7)", async () => {
    const err = await IndexPage({
      searchParams: Promise.resolve({
        account: ACCOUNT,
        tc: "rapid",
        color: "black",
        to: "2026-10-31",
      }),
    }).catch((e: unknown) => e);
    const digest = String((err as { digest?: string }).digest);
    expect(digest).toContain(
      `/app/areas/openings?account=${ACCOUNT}&tc=rapid&color=black&to=2026-10-31;`,
    );
  });
});
