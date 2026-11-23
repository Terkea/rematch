import { within } from "@testing-library/react";
import { JSDOM } from "jsdom";
import { renderToString } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { CoachScope } from "../../src/server/profile";
import type { StrengthsModel } from "../../src/server/profile/strengths";

// The strengths view (`/app/strengths`), rendered from synthetic read-model objects: what each
// section says when it has a claim, and the calm line when it has none. No row is the user's.

vi.mock("next/navigation", async (orig) => ({
  ...(await orig<object>()),
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn(), prefetch: vi.fn() }),
}));

const { StrengthsView, VERDICT_TEXT, OWN_BASELINE_TEXT, CRITICAL_TITLE, HARD_TITLE, diffText } =
  await import("../../src/app/(app)/app/strengths/strengths-view.js");
const report = await import("../../src/app/(app)/app/report/_sections/openings.js");

const ACCOUNT = "00000000-0000-4000-8000-0000000000a1";
const scope: CoachScope = {
  accountId: ACCOUNT,
  timeClass: "rapid",
  platform: "chesscom",
  username: "e2e-hero",
  options: [
    {
      accountId: ACCOUNT,
      platform: "chesscom",
      username: "e2e-hero",
      timeClass: "rapid",
      analysedGames: 214,
    },
    {
      accountId: ACCOUNT,
      platform: "chesscom",
      username: "e2e-hero",
      timeClass: "blitz",
      analysedGames: 61,
    },
  ],
};

const FEN = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";

function base(): StrengthsModel {
  return {
    side: "both",
    range: { from: null, to: null },
    analysedGames: 214,
    bySide: null,
    above: {
      mode: "own",
      trends: { claims: [], enough: true, measured: 214, needGames: 0 },
    },
    improving: { weaknesses: [], kpis: null },
    openings: { lines: [], promising: [], judged: 0 },
    hardSpots: { mode: "none", spots: [], notes: {} },
    difficulty: { positions: 40, pendingMaia: 40, ok: 0, notApplicable: 0 },
    report: null,
  };
}

function show(model: StrengthsModel | null, s: CoachScope | null = scope) {
  const html = renderToString(<StrengthsView scope={s} model={model} />);
  const body = new JSDOM(`<!doctype html><body>${html}</body>`).window.document.body;
  return { body, screen: within(body) };
}

const section = (body: HTMLElement, id: string) =>
  body.querySelector(`[data-section="${id}"]`) as HTMLElement;

describe("StrengthsView", () => {
  it("states the basis and the four sections, each calm when empty", () => {
    const { body, screen } = show(base());
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Strengths");
    expect(body.textContent).toContain("Based on 214 analysed rapid games on chess.com.");
    for (const h of [
      "Where you're above your level",
      "What's improving",
      "Openings that work for you",
      // No Maia rating yet: the fallback's title never says "hard" (N11).
      CRITICAL_TITLE,
    ]) {
      expect(screen.getByRole("heading", { level: 2, name: h })).toBeTruthy();
    }
    expect(section(body, "above").textContent).toContain(OWN_BASELINE_TEXT);
    expect(section(body, "above").textContent).toContain("Nothing stands out yet");
    expect(section(body, "improving").textContent).toContain(
      "No weakness has a confirmed improvement yet.",
    );
    expect(section(body, "openings").textContent).toContain(
      "Not enough games yet: a line needs at least 10 analysed games",
    );
    expect(section(body, "hard-spots").textContent).toContain(
      "Difficulty pending: Maia hasn't rated these positions yet (40 of 40 positions waiting).",
    );
    expect(body.textContent).not.toContain(HARD_TITLE);
    expect(body.textContent).not.toMatch(/NaN|undefined|Infinity|null/);
  });

  it("compares with the players faced when a peer benchmark exists, with its bias", () => {
    const m = base();
    m.above = {
      mode: "peers",
      compared: 4,
      benchmark: {
        source: "peers",
        platform: "chesscom",
        low: 650,
        high: 950,
        players: 25,
        games: 61,
        chances: {},
      },
      claims: [
        {
          metric: "accuracy",
          label: "Accuracy",
          unit: "pct",
          higherIsBetter: true,
          value: 76.2,
          low: 74.1,
          high: 78.3,
          n: 50,
          peer: 71.4,
          pooled: false,
          chances: null,
          players: 25,
        },
        {
          metric: "blunders_per_game",
          label: "Blunders per game",
          unit: "per_game",
          higherIsBetter: false,
          value: 0.9,
          low: 0.7,
          high: 1.1,
          n: 50,
          peer: 1.4,
          pooled: true,
          chances: 61,
          players: 25,
        },
      ],
    };
    const { body } = show(m);
    const above = section(body, "above");
    expect(above.textContent).toContain(
      "Compared with players you've faced on chess.com, rated about 650–950 (25 players).",
    );
    expect(above.textContent).toContain(
      "Only their games against you count, so it is a rough guide, not a rating-band average.",
    );
    expect(above.textContent).not.toContain(OWN_BASELINE_TEXT);
    expect(body.querySelector('[data-claim="accuracy"]')?.textContent).toContain(
      "80% range 74.1% to 78.3%, over your last 50 games. Players you've faced, rated about 650–950: 71.4% (the middle of 25 players' averages).",
    );
    expect(body.querySelector('[data-claim="blunders_per_game"]')?.textContent).toContain(
      "Players you've faced, rated about 650–950: 1.40 (pooled over 61 of their chances).",
    );
  });

  it("is calm when no measure beats the players faced", () => {
    const m = base();
    m.above = {
      mode: "peers",
      compared: 3,
      benchmark: {
        source: "peers",
        platform: "chesscom",
        low: 650,
        high: 950,
        players: 25,
        games: 61,
        chances: {},
      },
      claims: [],
    };
    const { body } = show(m);
    expect(section(body, "above").textContent).toContain(
      "None of your 3 measures compared with them is clearly better than theirs yet.",
    );
  });

  it("says how many earlier games the own baseline still needs", () => {
    const m = base();
    m.above = {
      mode: "own",
      trends: { claims: [], enough: false, measured: 62, needGames: 8 },
    };
    const { body } = show(m);
    expect(section(body, "above").textContent).toContain(
      "Not enough games yet. This compares your last 50 games with the games before them, so it needs 8 more analysed games (you have 62 games with these measures).",
    );
  });

  it("shows an own-baseline claim with both sides' n and the change's range", () => {
    const m = base();
    m.above = {
      mode: "own",
      trends: {
        enough: true,
        measured: 90,
        needGames: 0,
        claims: [
          {
            metric: "accuracy",
            label: "Accuracy",
            unit: "pct",
            higherIsBetter: true,
            recent: { value: 76.14, n: 50 },
            earlier: { value: 65, n: 40 },
            diff: { low: 9.1, high: 13.2 },
          },
        ],
      },
    };
    const { body } = show(m);
    const row = body.querySelector('[data-claim="accuracy"]');
    expect(row?.querySelector(".font-serif")?.textContent).toBe("76.1%");
    expect(row?.textContent).toContain(
      "76.1% over your last 50 games, against 65.0% over 40 earlier games. The change's 80% range is +9.1 points to +13.2 points.",
    );
    expect(section(body, "improving").textContent).toContain(
      `Measures that rose against your earlier games are under "Where you're above your level".`,
    );
  });

  it("compares with the band when a benchmark exists, and lists KPI trends under improving", () => {
    const m = base();
    m.above = {
      mode: "band",
      band: "B2",
      compared: 5,
      claims: [
        {
          metric: "blunders_per_game",
          label: "Blunders per game",
          unit: "per_game",
          higherIsBetter: false,
          value: 0.8,
          low: 0.6,
          high: 1.0,
          n: 50,
          median: 1.2,
          band: "B2",
        },
      ],
    };
    m.improving.kpis = { claims: [], enough: false, measured: 53, needGames: 17 };
    const { body } = show(m);
    expect(section(body, "above").textContent).not.toContain(OWN_BASELINE_TEXT);
    expect(body.querySelector('[data-claim="blunders_per_game"]')?.textContent).toContain(
      "80% range 0.60 to 1.00, over your last 50 games. Players in your rating band: 1.20.",
    );
    expect(section(body, "improving").textContent).toContain("needs 17 more analysed games");
  });

  it("shows a confirmed weakness change with its evidence, linked to its page", () => {
    const m = base();
    m.improving.weaknesses = [
      {
        seriesKey: "W1",
        label: "Hung material",
        state: "improving",
        before: 0.04,
        after: 0.026,
        pChange: 0.86,
        at: new Date("2026-10-14T12:00:00Z"),
        afterCounts: { events: 21, chances: 800 },
        chances: 2400,
      },
    ];
    const { body } = show(m);
    const row = body.querySelector('[data-claim="W1"]');
    expect(row?.textContent).toContain(
      "4.0% of chances before, 2.6% after, detected improving on 14 Oct 2026 (86% likely a real change). Since then: 21 times in 800 chances.",
    );
    expect(row?.querySelector("a")?.getAttribute("href")).toBe(
      `/app/weaknesses/W1?account=${ACCOUNT}&tc=rapid`,
    );
  });

  it("shows an opening's score against expected, with n and the report's verdict word", () => {
    const m = base();
    m.openings = {
      judged: 3,
      promising: [],
      lines: [
        {
          lineKey: "italian",
          name: "Italian Game",
          color: "white",
          kind: "chosen",
          verdict: "keep",
          n: 14,
          score: 0.643,
          expected: 0.51,
          low: 0.52,
          high: 0.75,
        },
      ],
    };
    const { body } = show(m);
    expect(body.querySelector('[data-claim="italian"]')?.textContent).toContain(
      "You play it as White. Scored 64% in 14 games (80% range 52% to 75%); your rating predicted 51%. Verdict: Keep.",
    );
  });

  it("names a promising line as likely above, not yet certain, with n and the expectation", () => {
    const m = base();
    m.openings = {
      judged: 3,
      lines: [],
      promising: [
        {
          lineKey: "caro-kann",
          name: "Caro-Kann Defence",
          color: "black",
          kind: "chosen",
          verdict: null,
          n: 12,
          score: 0.667,
          expected: 0.52,
          pAbove: 0.861,
        },
      ],
    };
    const { body, screen } = show(m);
    const openings = section(body, "openings");
    expect(openings.textContent).toContain(
      "None of your 3 lines with 10 or more games scores clearly above what your rating predicts yet.",
    );
    expect(within(openings).getByRole("heading", { level: 3, name: "Promising" })).toBeTruthy();
    expect(body.querySelector('[data-claim="caro-kann"]')?.textContent).toContain(
      "You play it as Black. Scored 67% in 12 games; your rating predicted 52%. 86% likely above it, not yet certain.",
    );
    expect(screen.getByText(/At least 80% likely to score above/)).toBeTruthy();
  });

  it("uses the report's verdict words", () => {
    expect(VERDICT_TEXT).toEqual(report.VERDICT_TEXT);
  });

  it("shows a found move in a position explorer, labels the fallback and links to that ply", () => {
    const m = base();
    m.hardSpots = {
      mode: "critical",
      spots: [
        {
          gameId: "g-1",
          playedAt: new Date("2026-10-03T12:00:00Z"),
          ply: 5,
          kind: "only_move_found",
          fen: FEN,
          san: "Bb5",
          uci: "f1b5",
          bestUci: "f1b5",
          winBefore: 54,
          winAfter: 55,
          timeSpent: 12,
          difficultyStatus: "pending_maia",
          pBest: null,
          isCritical: true,
          multipvGap: 22.4,
          prevSan: "Nc6",
          prevJudgement: null,
        },
      ],
      notes: { only_move_found: "Keep doing this: when the position is sharp, take your time." },
    };
    const { body } = show(m);
    const hard = section(body, "hard-spots");
    expect(within(hard).getByRole("heading", { level: 2 }).textContent).toBe(CRITICAL_TITLE);
    expect(hard.textContent).toContain(
      "Until it does, these are moments where the engine's best move was at least 15 win% points better than its second choice, your win chance was between 25% and 75%, and you found it.",
    );
    const spot = body.querySelector('[data-spot="g-1:5"]') as HTMLElement;
    expect(spot.querySelector("[data-position-explorer]")).toBeTruthy();
    expect(
      within(spot).getByRole("button", { name: "Expand the position before 3. Bb5" }),
    ).toBeTruthy();
    expect(spot.textContent).toContain("Played, Bb5, the engine's move");
    expect(spot?.textContent).toContain(
      "The engine's next choice was 22 win% points worse. Win chance 54% to 55%. You took 12 s.",
    );
    expect(spot?.textContent).toContain("Keep doing this: when the position is sharp");
    expect(
      within(spot).getByRole("link", { name: "Open the game at 3. Bb5" }).getAttribute("href"),
    ).toBe("/app/games/g-1?ply=5");
    expect(body.textContent).not.toMatch(/NaN|undefined|Infinity/);
  });

  it("names the report's strengths so the page and the coach agree", () => {
    const m = base();
    m.report = {
      at: new Date("2026-10-13T12:00:00Z"),
      strengths: [
        { area: "openings", label: "Solid Italian repertoire", metricKey: null, examples: 1 },
      ],
    };
    const { body } = show(m);
    expect(body.querySelector("[data-report-strengths]")?.textContent).toContain(
      "Your report of 13 Oct 2026 names this strength, as your coach does:Solid Italian repertoire (Openings)",
    );
  });

  it("is one calm empty sheet without analysed games", () => {
    const { body } = show(null, null);
    expect(body.textContent).toContain("No analysed games yet.");
    expect(body.querySelector("[data-section]")).toBeNull();
  });
});

describe("diffText", () => {
  it("signs with a real minus and states the unit", () => {
    expect(diffText(-0.4, "per_game")).toBe("−0.40 a game");
    expect(diffText(0.12, "rate")).toBe("+12.0 percentage points");
    expect(diffText(Number.NaN, "pct")).toBe("not measured");
  });
});
