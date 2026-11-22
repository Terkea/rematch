// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const push = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push, replace: vi.fn(), refresh: vi.fn() }),
}));

const { CoachScopeSwitch, scopeOptionLabel } = await import(
  "../../src/components/coach-scope-switch.js"
);

const lichess = {
  accountId: "11111111-1111-4111-8111-111111111111",
  platform: "lichess" as const,
  username: "knightly",
};
const chesscom = {
  accountId: "22222222-2222-4222-8222-222222222222",
  platform: "chesscom" as const,
  username: "knightly",
};

beforeEach(() => push.mockReset());

describe("CoachScopeSwitch", () => {
  it("one option renders nothing", () => {
    const { container } = render(
      <CoachScopeSwitch
        basePath="/app/report"
        scope={{
          ...lichess,
          timeClass: "rapid",
          options: [{ ...lichess, timeClass: "rapid", analysedGames: 214 }],
        }}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("labels each option with its count", () => {
    expect(scopeOptionLabel({ ...lichess, timeClass: "rapid", analysedGames: 214 }, false)).toBe(
      "Rapid on lichess (214 games)",
    );
    expect(scopeOptionLabel({ ...chesscom, timeClass: "blitz", analysedGames: 1 }, false)).toBe(
      "Blitz on chess.com (1 game)",
    );
    expect(scopeOptionLabel({ ...chesscom, timeClass: "blitz", analysedGames: 1200 }, true)).toBe(
      "Blitz on chess.com as knightly (1,200 games)",
    );
  });

  it("choosing an option writes account and tc", async () => {
    const user = userEvent.setup();
    render(
      <CoachScopeSwitch
        basePath="/app/weaknesses"
        scope={{
          ...lichess,
          timeClass: "rapid",
          options: [
            { ...lichess, timeClass: "rapid", analysedGames: 214 },
            { ...chesscom, timeClass: "blitz", analysedGames: 88 },
          ],
        }}
      />,
    );
    const rapid = screen.getByRole("radio", { name: "Rapid on lichess (214 games)" });
    expect(rapid).toBeChecked();
    await user.click(screen.getByRole("radio", { name: "Blitz on chess.com (88 games)" }));
    expect(push).toHaveBeenCalledWith(`/app/weaknesses?account=${chesscom.accountId}&tc=blitz`, {
      scroll: false,
    });
  });
});
