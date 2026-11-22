// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { isValidElement, type ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { game } from "../helpers/review-game";

// The page: scoped by T3's getGameReview, not found for anything it returns null for, the header
// on the server, and only the narrow view model across the client boundary.

const full = {
  ...game(["e4", "e5"], { 1: { winWhite: 55, judgement: "inaccuracy", deltaW: 6 } }),
  sourceUrl: "https://www.chess.com/game/live/1",
  eco: "C20",
  openingName: "King's Pawn Game",
  termination: "Blacky won on time",
  playerResult: "loss" as const,
};
const getGameReview = vi.fn();
const requireUser = vi.fn(async () => ({ id: "u1", email: "x@example.invalid" }));
vi.mock("../../src/server/games/review", async (orig) => ({
  ...(await orig<object>()),
  getGameReview,
}));
vi.mock("../../src/server/auth/session", () => ({ requireUser }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));

const mod = await import("../../src/app/(app)/app/games/[gameId]/page.js");
const Page = mod.default;
const ID = "00000000-0000-4000-8000-000000000001";
const args = (ply?: string) => ({
  params: Promise.resolve({ gameId: ID }),
  searchParams: Promise.resolve(ply === undefined ? {} : { ply }),
});

beforeEach(() => {
  getGameReview.mockReset();
  getGameReview.mockResolvedValue(full);
});

function findView(node: unknown): Record<string, unknown> | null {
  if (!isValidElement(node)) {
    if (Array.isArray(node))
      for (const n of node) {
        const hit = findView(n);
        if (hit) return hit;
      }
    return null;
  }
  const props = node.props as Record<string, unknown>;
  if ("view" in props) return props;
  return findView(props.children);
}

describe("game review page", () => {
  it("reads the game as the signed-in user, scoped by getGameReview", async () => {
    await Page(args());
    expect(requireUser).toHaveBeenCalled();
    expect(getGameReview).toHaveBeenCalledWith({}, "u1", ID);
  });

  it("is not found when the game isn't the user's (or doesn't exist)", async () => {
    getGameReview.mockResolvedValue(null);
    await expect(Page(args())).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
  });

  it("hands the client view the narrow model and the ply param only", async () => {
    const tree = (await Page(args("7"))) as ReactElement;
    const props = findView(tree);
    expect(Object.keys(props ?? {}).sort()).toEqual(["initialPly", "view"]);
    expect(props?.initialPly).toBe("7");
    const json = JSON.stringify(props?.view);
    for (const leak of ["chess.com/game", "C20", "King's Pawn", "won on time", ID])
      expect(json).not.toContain(leak);
  });

  it("renders the header on the server", async () => {
    const tree = (await Page(args())) as ReactElement;
    // Render only the header and the result (the first two children): the view is a client one.
    const [header, result] = (tree.props as { children: ReactElement[] }).children;
    render(
      <>
        {header}
        {result}
      </>,
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Whitey v Blacky");
    expect(screen.getByText("Rapid 10+0, rated, 7 Nov 2026")).toBeInTheDocument();
    expect(screen.getByText("You lost")).toBeInTheDocument();
    expect(screen.getByText("Blacky won on time")).toBeInTheDocument();
    expect(screen.getByText("Your accuracy 70.0%")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "View on chess.com" })).toHaveAttribute(
      "href",
      "https://www.chess.com/game/live/1",
    );
  });

  it("titles the document", async () => {
    const meta = await mod.generateMetadata(args());
    expect(meta.title).toBe("Whitey v Blacky, 7 Nov 2026");
  });
});
