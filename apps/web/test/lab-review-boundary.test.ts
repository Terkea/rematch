// @vitest-environment jsdom
import { isValidElement, type ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { game } from "./helpers/review-game";

// Task 7 fix round 1: the client gets a narrow view model with only what each variant renders,
// never the raw ReviewGame (no id, source URL, platform, opening or result fields it doesn't show).
// And the production 404 does not rest on the layout alone: loading refuses before any auth or DB.

const full = {
  ...game(["e4", "e5", "Nf3"], {
    1: { winWhite: 55, judgement: "inaccuracy", classification: "inaccuracy", deltaW: 3 },
  }),
  sourceUrl: "https://www.chess.com/game/live/1",
  eco: "C20",
  openingName: "King's Pawn Game",
  termination: "Blacky won on time",
};

const loadLabReview = vi.fn(async () => full);
const requireUser = vi.fn();
vi.mock("../src/app/lab/review/load", async (orig) => ({
  ...(await orig<object>()),
  loadLabReview,
}));
vi.mock("../src/server/auth/session", () => ({ requireUser }));

const { default: PageA } = await import("../src/app/lab/review/a/page.js");
const { default: PageB } = await import("../src/app/lab/review/b/page.js");
const { default: PageC } = await import("../src/app/lab/review/c/page.js");

const TOP = [
  "white",
  "black",
  "playedAt",
  "timeClass",
  "baseSeconds",
  "incrementSeconds",
  "rated",
  "playerColor",
  "playerResult",
  "termination",
  "accuracy",
  "startFen",
  "startScore",
  "analysed",
  "hasClocks",
  "phases",
  "moments",
  "plies",
];
const PLY = ["ply", "color", "san", "uci", "fenAfter", "clockAfter", "analysis"];
const ANALYSIS = [
  "winWhite",
  "score",
  "scoreSource",
  "judgement",
  "classification",
  "bestUci",
  "timeSpent",
];

/** The props the page hands its client view (the LabFrame's only child). */
async function clientProps(Page: typeof PageA): Promise<Record<string, unknown>> {
  const tree = (await Page({ searchParams: Promise.resolve({}) })) as ReactElement<{
    children: ReactElement;
  }>;
  const child = tree.props.children;
  expect(isValidElement(child)).toBe(true);
  return child.props as Record<string, unknown>;
}

afterEach(() => vi.unstubAllEnvs());

describe("client boundary", () => {
  for (const [name, Page, drops] of [
    ["a", PageA, false],
    ["b", PageB, true],
    ["c", PageC, true],
  ] as const) {
    it(`variant ${name} passes only the fields it renders`, async () => {
      const props = await clientProps(Page);
      expect(Object.keys(props)).toEqual(["view"]);
      const view = props.view as { plies: { analysis: object | null }[] };
      expect(Object.keys(view).sort()).toEqual([...TOP].sort());
      for (const p of view.plies) expect(Object.keys(p).sort()).toEqual([...PLY].sort());
      const analysis = view.plies[0]?.analysis ?? {};
      // Only the variants with a commentary line or mistake list get the win% drop.
      expect(Object.keys(analysis).sort()).toEqual(
        [...ANALYSIS, ...(drops ? ["deltaW"] : [])].sort(),
      );
      const json = JSON.stringify(view);
      for (const leak of ["chess.com/game", "C20", "King's Pawn", full.id, "chesscom"])
        expect(json).not.toContain(leak);
    });
  }
});

describe("production guard in the loader", () => {
  it("is not found in production before any auth or database work", async () => {
    vi.stubEnv("NODE_ENV", "production");
    vi.stubEnv("LAB_ENABLED", "");
    const { loadLabReview: real } = await vi.importActual<
      typeof import("../src/app/lab/review/load")
    >("../src/app/lab/review/load");
    await expect(real()).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    expect(requireUser).not.toHaveBeenCalled();
  });
});
