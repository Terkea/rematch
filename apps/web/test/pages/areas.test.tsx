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

const ENDGAME_VIEW: AreaView = {
  area: "endgame",
  score: { value: 61.6, n: 46, computedAt: AT },
  kpis: [],
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
      { week: "2026-11-02", value: 70.2, low: null, high: null, n: 9 },
      { week: "2026-11-09", value: 72.8, low: null, high: null, n: 12 },
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
    points: [{ week: "2026-11-09", value: 0.5, low: null, high: null, n: 2 }],
  }),
  trend({
    metric: "tb_errors_per_endgame",
    label: "Tablebase errors per endgame",
    unit: "per_game",
  }),
];

const detail = (w: WeaknessSummary): WeaknessDetail => ({
  ...w,
  history: [],
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
const requireUser = vi.fn(async () => ({ id: "u1", email: "x@example.invalid" }));

vi.mock("../../src/server/auth/session", () => ({ requireUser }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));
vi.mock("../../src/server/profile", async (orig) => ({
  ...(await orig<object>()),
  resolveCoachScope,
  getArea,
  kpiTrends,
  getWeakness,
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

describe("skill-area pages", () => {
  it("unknown area is not found", async () => {
    await expect(Page(args("kingside"))).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    expect(getArea).not.toHaveBeenCalled();
  });

  it("reads through the read models as the signed-in user, in the resolved scope", async () => {
    await show("endgame", { account: ACCOUNT, tc: "rapid" });
    expect(resolveCoachScope).toHaveBeenCalledWith({}, "u1", { account: ACCOUNT, tc: "rapid" });
    expect(getArea).toHaveBeenCalledWith({}, "u1", SCOPE, "endgame");
    expect(getReport).toHaveBeenCalledWith({}, "u1", { accountId: ACCOUNT, timeClass: "rapid" });
    for (const call of [...kpiTrends.mock.calls, ...getWeakness.mock.calls]) {
      expect(call[1]).toBe("u1");
      expect(call[2]).toBe(SCOPE);
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
      kpis: [],
      weaknesses: [],
      timeline: [],
    });
    const { container } = await show("time");
    const score = within(screen.getByRole("region", { name: "Score" }));
    expect(score.getByText("Not enough games for a score yet")).toBeInTheDocument();
    expect(score.getByText(/6 analysed rapid games so far/)).toBeInTheDocument();
    expect(container.querySelector("[data-area-label]")).toBeNull();
    expect(screen.getByRole("region", { name: "Score" }).textContent).not.toMatch(/\b0\b/);
  });

  it("needMore from the report is shown", async () => {
    const p = payload();
    const row = p.computed.areaScores.find((a) => a.area === "endgame");
    if (!row) throw new Error("fixture changed");
    Object.assign(row, { score: null, sufficient: false, needMore: 7, n: 13 });
    getReport.mockResolvedValue(reportView(p));
    getArea.mockResolvedValue({ ...ENDGAME_VIEW, score: null });
    await show("endgame");
    expect(screen.getByText("Need 7 more games for an endgame score")).toBeInTheDocument();
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

describe("/app/areas", () => {
  it("redirects to openings, keeping the scope params", async () => {
    const err = await IndexPage({
      searchParams: Promise.resolve({ account: ACCOUNT, tc: "rapid", other: "x" }),
    }).catch((e: unknown) => e);
    const digest = String((err as { digest?: string }).digest);
    expect(digest).toContain(`/app/areas/openings?account=${ACCOUNT}&tc=rapid;`);
    expect(digest).not.toContain("other");
  });
});
