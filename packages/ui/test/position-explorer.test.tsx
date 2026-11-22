import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { games } from "../src/fixtures/index.js";
import {
  type ExplorerMove,
  PositionExplorer,
  type PositionExplorerProps,
  type PositionLine,
} from "../src/position-explorer/index.js";

afterEach(cleanup);

const BAD = /NaN|undefined|Infinity|null/;

// 28. Nd2?? from the fixture DrNykterstein v Night-King96 (ply 55, White, the player's side).
const game = games.middlegameBlunder;
const fenBefore = (ply: number) =>
  ply === 1 ? game.startFen : (game.plies[ply - 2]?.fenAfter ?? "");
const asMove = (ply: number): ExplorerMove => {
  const p = game.plies[ply - 1];
  if (!p) throw new Error("fixture changed");
  return {
    ply,
    color: p.color,
    san: p.san,
    uci: p.uci,
    fenBefore: fenBefore(ply),
    fenAfter: p.fenAfter,
  };
};
const CITED = 55;
const line: PositionLine = { game: [51, 52, 53, 54, 55, 56, 57, 58, 59].map(asMove), engine: [] };

const props: PositionExplorerProps = {
  gameId: "game-1",
  ply: CITED,
  fenBefore: fenBefore(CITED),
  played: { san: "Nd2", uci: "e4d2" },
  best: null,
  orientation: "white",
  label: "28. Nd2",
  mistake: true,
  href: "/app/games/game-1?ply=55",
};

async function expand(p: Partial<PositionExplorerProps> = {}) {
  const user = userEvent.setup();
  const view = render(<PositionExplorer {...props} {...p} />);
  const button = screen.getByRole("button", { name: "Expand the position before 28. Nd2" });
  await user.click(button);
  const dialog = await screen.findByRole("dialog", { name: "Position before 28. Nd2" });
  return { user, view, button, dialog };
}

const current = (dialog: HTMLElement) =>
  dialog.querySelector("[aria-current='step']")?.getAttribute("data-ply") ?? null;

describe("PositionExplorer, compact", () => {
  it("shows the small board, its key and the way into the game", async () => {
    render(<PositionExplorer {...props} />);
    expect(await screen.findByRole("group", { name: "Position before 28. Nd2" })).toBeVisible();
    expect(screen.getByText("Played, Nd2")).toBeVisible();
    expect(screen.getByRole("link", { name: "Open the game at 28. Nd2" })).toHaveAttribute(
      "href",
      props.href,
    );
    expect(
      screen.getByRole("button", { name: "Expand the position before 28. Nd2" }),
    ).toBeVisible();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("names the engine's better move when there is one", async () => {
    render(
      <PositionExplorer
        {...props}
        ply={1}
        fenBefore={game.startFen}
        played={{ san: "d4", uci: "d2d4" }}
        best={{ san: "e4", uci: "e2e4" }}
        label="1. d4"
        mistake={false}
      />,
    );
    expect(
      await screen.findByRole("group", { name: "Position before 1. d4; engine preferred e4" }),
    ).toBeVisible();
    expect(screen.getByText("Better, e4")).toBeVisible();
  });
});

describe("PositionExplorer, expanded", () => {
  it("opens a dialog on the position before the move, loading the line once", async () => {
    const loadLine = vi.fn(async () => line);
    const { dialog, user, button } = await expand({ loadLine });
    expect(loadLine).toHaveBeenCalledWith("game-1", CITED);
    await waitFor(() => expect(current(dialog)).toBe("54"));
    expect(within(dialog).getByRole("group", { name: /^Position before 28\. Nd2/ })).toBeVisible();
    expect(dialog.textContent).not.toMatch(BAD);

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(button).toHaveFocus();
    await user.click(button);
    await screen.findByRole("dialog");
    expect(loadLine).toHaveBeenCalledTimes(1);
  });

  it("the board opens it too", async () => {
    const user = userEvent.setup();
    const { container } = render(<PositionExplorer {...props} line={line} />);
    const opener = container.querySelector("[data-explorer-open]");
    if (!opener) throw new Error("no board opener");
    await user.click(opener);
    expect(await screen.findByRole("dialog")).toBeVisible();
  });

  it("steps through the game with the buttons and the arrow keys", async () => {
    const { dialog, user } = await expand({ line });
    await user.click(within(dialog).getByRole("button", { name: "Next move" }));
    expect(current(dialog)).toBe("55");
    fireEvent.keyDown(dialog, { key: "ArrowRight" });
    expect(current(dialog)).toBe("56");
    fireEvent.keyDown(dialog, { key: "ArrowLeft" });
    fireEvent.keyDown(dialog, { key: "ArrowLeft" });
    expect(current(dialog)).toBe("54");
    fireEvent.keyDown(dialog, { key: "Home" });
    expect(within(dialog).getByRole("button", { name: "Previous move" })).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "End" });
    expect(current(dialog)).toBe("59");
    expect(within(dialog).getByRole("button", { name: "Next move" })).toBeDisabled();
  });

  it("switches to the engine line, and says so when none is stored", async () => {
    const engineLine: PositionLine = {
      ...line,
      engine: [
        {
          ...asMove(CITED),
          san: "Nc3",
          uci: "e4c3",
        },
      ],
    };
    const { dialog, user } = await expand({ line: engineLine });
    await user.click(within(dialog).getByRole("radio", { name: "Engine line" }));
    await user.click(within(dialog).getByRole("button", { name: "Next move" }));
    expect(within(dialog).getByRole("button", { name: /Nc3/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
    cleanup();
    const again = await expand({ line });
    await again.user.click(within(again.dialog).getByRole("radio", { name: "Engine line" }));
    expect(within(again.dialog).getByText("No engine line is stored for this move.")).toBeVisible();
  });

  it("tries a legal move by clicking squares, honestly unchecked, and resets", async () => {
    const { dialog, user } = await expand({ line });
    const square = (s: string) => {
      const el = dialog.querySelector(`[data-square-button='${s}']`);
      if (!el) throw new Error(`no square ${s}`);
      return el as HTMLElement;
    };
    // The keyboard plays it: Enter on e4, two left and one down to c3, Enter.
    square("e4").focus();
    await user.keyboard("{Enter}{ArrowLeft}{ArrowLeft}{ArrowDown}{Enter}");
    expect(within(dialog).getByRole("radio", { name: "Try a move" })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: /Nc3/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(within(dialog).getByText(/not checked by the engine/)).toBeVisible();
    expect(within(dialog).getByRole("link", { name: "Open full game" })).toHaveAttribute(
      "href",
      props.href,
    );
    await user.click(within(dialog).getByRole("button", { name: "Reset" }));
    expect(within(dialog).queryByRole("button", { name: /Nc3/ })).toBeNull();
    expect(current(dialog)).toBe("54");
  });

  it("says when the tried move is the one played in the game", async () => {
    const { dialog, user } = await expand({ line });
    (dialog.querySelector("[data-square-button='e4']") as HTMLElement).focus();
    await user.keyboard("{Enter}{ArrowLeft}{ArrowDown}{ArrowDown}{Enter}");
    expect(within(dialog).getByText("That is the move played in the game.")).toBeVisible();
  });

  it("flips the board", async () => {
    const { dialog, user } = await expand({ line });
    const board = () => dialog.querySelector("[data-board]");
    expect(board()).toHaveAttribute("data-orientation", "white");
    await user.click(within(dialog).getByRole("button", { name: "Flip board" }));
    expect(board()).toHaveAttribute("data-orientation", "black");
  });

  it("keeps the cited moves when the rest of the game cannot be loaded", async () => {
    const loadLine = vi.fn(async () => {
      throw new Error("offline");
    });
    const { dialog } = await expand({ loadLine });
    expect(await within(dialog).findByText(/Couldn't load the rest of the game/)).toBeVisible();
    await act(async () => {
      fireEvent.keyDown(dialog, { key: "ArrowRight" });
    });
    expect(current(dialog)).toBe("55");
  });
});
