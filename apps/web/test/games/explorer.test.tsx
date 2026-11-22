// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseGameQuery } from "../../src/server/games/query.js";
import type { GameRow as GameRowData } from "../../src/server/games/types.js";

const replace = vi.fn();
vi.mock("next/navigation", () => ({
  usePathname: () => "/app/games",
  useRouter: () => ({ push: vi.fn(), replace, refresh: vi.fn(), prefetch: vi.fn() }),
}));

const { FilterBar } = await import("../../src/app/(app)/app/games/_explorer/filter-bar.js");
const { GameRow, toRowView } = await import("../../src/app/(app)/app/games/_explorer/game-row.js");
const { Pagination } = await import("../../src/app/(app)/app/games/_explorer/pagination.js");
const { GamesList, showingText } = await import(
  "../../src/app/(app)/app/games/_explorer/games-list.js"
);
const { NoGames, NoMatch } = await import("../../src/app/(app)/app/games/_explorer/empty.js");
const { describeFilters, parseDayText } = await import(
  "../../src/app/(app)/app/games/_explorer/href.js"
);

beforeEach(() => replace.mockReset());

const q = (p: Record<string, string>) => parseGameQuery(p);

/** A synthetic row (never the owner's data). */
function row(over: Partial<GameRowData> = {}): GameRowData {
  return {
    id: "00000000-0000-4000-8000-000000000001",
    accountId: "00000000-0000-4000-8000-0000000000aa",
    platform: "chesscom",
    playedAt: new Date("2026-11-12T18:30:00Z"),
    timeClass: "rapid",
    baseSeconds: 600,
    incrementSeconds: 0,
    rated: true,
    playerColor: "white",
    player: { name: "someplayer", rating: 1480 },
    opponent: { name: "Opponent", rating: 1520 },
    result: "win",
    termination: "resignation",
    eco: "C65",
    openingName: "Ruy Lopez: Berlin Defence",
    analysisStatus: "done",
    accuracy: 71.3,
    counts: { blunders: 2, mistakes: 1, inaccuracies: 3 },
    plies: 64,
    ...over,
  };
}

describe("filter bar", () => {
  it("shows Clear filters when a filter is set", () => {
    render(<FilterBar query={q({ tc: "rapid" })} accounts={[]} />);
    expect(screen.getByRole("button", { name: "Clear filters" })).toBeInTheDocument();
  });

  it("hides Clear filters with no filter set, even when sorted", () => {
    render(<FilterBar query={q({ sort: "accuracy-desc" })} accounts={[]} />);
    expect(screen.queryByRole("button", { name: "Clear filters" })).toBeNull();
  });

  it("replaces the URL with the new filter and resets the page", async () => {
    const user = userEvent.setup();
    render(<FilterBar query={q({ tc: "rapid", page: "3" })} accounts={[]} />);
    await user.click(screen.getByRole("radio", { name: "Losses" }));
    expect(replace).toHaveBeenCalledWith("/app/games?tc=rapid&result=loss", { scroll: false });
  });

  it("Clear filters keeps the sort and drops every filter", async () => {
    const user = userEvent.setup();
    render(
      <FilterBar
        query={q({ tc: "rapid", colour: "black", from: "2026-01-01", sort: "blunders-desc" })}
        accounts={[]}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(replace).toHaveBeenCalledWith("/app/games?sort=blunders-desc", { scroll: false });
  });

  it("changing a date filters from that day", async () => {
    render(<FilterBar query={q({})} accounts={[]} />);
    const from = screen.getByLabelText("From");
    const { fireEvent } = await import("@testing-library/react");
    fireEvent.change(from, { target: { value: "2026-10-01" } });
    expect(replace).toHaveBeenCalledWith("/app/games?from=2026-10-01", { scroll: false });
  });

  it("waits for a whole year while a date is typed (no navigation on 0002-10-01)", async () => {
    render(<FilterBar query={q({})} accounts={[]} />);
    const { fireEvent } = await import("@testing-library/react");
    const from = screen.getByLabelText("From");
    fireEvent.change(from, { target: { value: "0002-10-01" } });
    expect(replace).not.toHaveBeenCalled();
    expect(from).toHaveValue("0002-10-01");
    fireEvent.change(from, { target: { value: "2026-10-01" } });
    expect(replace).toHaveBeenCalledWith("/app/games?from=2026-10-01", { scroll: false });
  });

  it("keeps the same date input (and its focus) after a date navigates", async () => {
    const { rerender } = render(<FilterBar query={q({})} accounts={[]} />);
    const { fireEvent } = await import("@testing-library/react");
    const from = screen.getByLabelText("From");
    from.focus();
    fireEvent.change(from, { target: { value: "2026-10-01" } });
    rerender(<FilterBar query={q({ from: "2026-10-01" })} accounts={[]} />);
    expect(screen.getByLabelText("From")).toBe(from);
    expect(from).toHaveFocus();
    // Once the day is in the URL, the field reads it in the app's words (T13).
    expect(from).toHaveValue("1 Oct 2026");
  });

  // M5a T13 critic I5: the native date input showed a US "mm/dd/yyyy"; the app writes
  // "1 Oct 2026" everywhere, so the field reads and accepts that, without the browser's chrome.
  it("reads a date filter in the app's words, and takes them typed", async () => {
    const { fireEvent } = await import("@testing-library/react");
    render(<FilterBar query={q({ to: "2026-10-31" })} accounts={[]} />);
    const from = screen.getByLabelText("From");
    const to = screen.getByLabelText("To");
    expect(from).toHaveAttribute("type", "text");
    expect(from).toHaveAttribute("placeholder", "e.g. 1 Oct 2026");
    expect(to).toHaveValue("31 Oct 2026");
    fireEvent.change(from, { target: { value: "1 Oct 2026" } });
    expect(replace).toHaveBeenCalledWith("/app/games?from=2026-10-01&to=2026-10-31", {
      scroll: false,
    });
  });

  it("says how to write a date it can't read, and navigates nowhere", async () => {
    const { fireEvent } = await import("@testing-library/react");
    render(<FilterBar query={q({})} accounts={[]} />);
    const from = screen.getByLabelText("From");
    from.focus();
    fireEvent.change(from, { target: { value: "31 Feb 2026" } });
    fireEvent.blur(from);
    expect(replace).not.toHaveBeenCalled();
    expect(from).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Write a date like 1 Oct 2026.")).toBeInTheDocument();
  });

  it("parses the day forms people type", () => {
    expect(parseDayText("2026-10-01")).toBe("2026-10-01");
    expect(parseDayText("1 Oct 2026")).toBe("2026-10-01");
    expect(parseDayText(" 01 october 2026 ")).toBe("2026-10-01");
    expect(parseDayText("1/10/2026")).toBe("2026-10-01");
    expect(parseDayText("31 Feb 2026")).toBeNull();
    expect(parseDayText("Oct 2026")).toBeNull();
    expect(parseDayText("1 Oct 26")).toBeNull();
    expect(parseDayText("")).toBeNull();
  });

  it("offers Account only with more than one link", () => {
    const { rerender } = render(
      <FilterBar query={q({})} accounts={[{ id: "a", label: "chess.com someplayer" }]} />,
    );
    expect(screen.queryByRole("combobox", { name: "Account" })).toBeNull();
    rerender(
      <FilterBar
        query={q({})}
        accounts={[
          { id: "00000000-0000-4000-8000-0000000000aa", label: "chess.com someplayer" },
          { id: "00000000-0000-4000-8000-0000000000bb", label: "Lichess someplayer" },
        ]}
      />,
    );
    expect(screen.getByRole("combobox", { name: "Account" })).toBeInTheDocument();
  });

  it("labels every control with the brief's copy", () => {
    render(<FilterBar query={q({})} accounts={[]} />);
    expect(screen.getByRole("combobox", { name: "Time control" })).toHaveTextContent(
      "All time controls",
    );
    expect(screen.getByRole("radiogroup", { name: "Result" })).toBeInTheDocument();
    const colour = screen.getByRole("radiogroup", { name: "Colour" });
    expect(within(colour).getAllByRole("radio")).toHaveLength(3);
    for (const name of ["Both", "White", "Black"]) {
      expect(within(colour).getByRole("radio", { name })).toBeInTheDocument();
    }
    expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveTextContent("Newest first");
    expect(screen.getByLabelText("To")).toBeInTheDocument();
  });
});

describe("game row", () => {
  it("pending row shows status", () => {
    render(
      <ul>
        <GameRow
          row={toRowView(row({ analysisStatus: "analysing", accuracy: null, counts: null }))}
        />
      </ul>,
    );
    expect(screen.getByText("Waiting for analysis")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("%");
  });

  it("maps failed and skipped to their status words", () => {
    expect(
      toRowView(row({ analysisStatus: "failed", accuracy: null, counts: null })).analysis,
    ).toEqual({
      kind: "status",
      text: "Couldn't analyse",
    });
    expect(
      toRowView(row({ analysisStatus: "skipped", accuracy: null, counts: null })).analysis,
    ).toEqual({
      kind: "status",
      text: "Skipped",
    });
  });

  it("is one link to the review with the game's fields", () => {
    render(
      <ul>
        <GameRow row={toRowView(row())} />
      </ul>,
    );
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(1);
    const link = links[0] as HTMLElement;
    expect(link).toHaveAttribute("href", "/app/games/00000000-0000-4000-8000-000000000001");
    expect(link).toHaveAccessibleName(
      "Win v Opponent (1520), you played White, Rapid 10+0, 12 Nov 2026, Ruy Lopez: Berlin Defence, accuracy 71.3%, 2 blunders, 1 mistake",
    );
    const text = document.body.textContent ?? "";
    for (const part of [
      "Opponent",
      "1520",
      "Win",
      "Rapid 10+0",
      "12 Nov 2026",
      "Ruy Lopez: Berlin Defence",
      "71.3%",
    ]) {
      expect(text).toContain(part);
    }
  });

  it("writes singular counts, odd time controls and an unrated opponent honestly", () => {
    const v = toRowView(
      row({
        counts: { blunders: 1, mistakes: 0, inaccuracies: 0 },
        timeClass: "bullet",
        baseSeconds: 30,
        incrementSeconds: 1,
        opponent: { name: "Anon", rating: null },
        result: "unfinished",
        openingName: null,
        eco: null,
      }),
    );
    expect(v.analysis).toEqual({
      kind: "accuracy",
      accuracy: "71.3%",
      counts: "1 blunder, 0 mistakes",
      blunders: 1,
      mistakes: 0,
    });
    expect(v.timeControl).toBe("Bullet ½+1");
    expect(v.opponentRating).toBeNull();
    expect(v.result).toBe("Unfinished");
    expect(v.opening).toBeNull();
    expect(
      toRowView(row({ timeClass: "daily", baseSeconds: 86400, incrementSeconds: 0 })).timeControl,
    ).toBe("Daily, 1 day a move");
    expect(toRowView(row({ baseSeconds: null, incrementSeconds: null })).timeControl).toBe("Rapid");
  });

  it("carries only what is rendered (narrow view)", () => {
    const v = toRowView(row());
    const json = JSON.stringify(v);
    expect(json).not.toContain("someplayer");
    expect(json).not.toContain(row().accountId);
    expect(json).not.toContain("resignation");
  });
});

describe("pagination", () => {
  it("renders no links on page 1 of 1", () => {
    render(<Pagination query={parseGameQuery({})} page={1} pageCount={1} />);
    expect(screen.queryAllByRole("link")).toHaveLength(0);
  });

  it("links to the neighbouring pages and keeps the filters", () => {
    render(
      <Pagination
        query={parseGameQuery({ tc: "blitz", sort: "accuracy-asc" })}
        page={2}
        pageCount={17}
      />,
    );
    expect(screen.getByText("Page 2 of 17")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Previous page" })).toHaveAttribute(
      "href",
      "/app/games?tc=blitz&sort=accuracy-asc",
    );
    expect(screen.getByRole("link", { name: "Next page" })).toHaveAttribute(
      "href",
      "/app/games?tc=blitz&sort=accuracy-asc&page=3",
    );
  });

  // M5a T13 critic polish 3: the missing step was grey text that read almost as a live link.
  it("keeps the missing step's place but does not draw it", () => {
    const { container } = render(<Pagination query={q({})} page={1} pageCount={3} />);
    const dead = container.querySelector("span[aria-hidden='true']");
    expect(dead).toHaveTextContent("Previous page");
    expect(dead).toHaveClass("invisible");
  });

  it("has no Next link on the last page", () => {
    render(<Pagination query={parseGameQuery({})} page={17} pageCount={17} />);
    expect(screen.queryByRole("link", { name: "Next page" })).toBeNull();
    expect(screen.getByRole("link", { name: "Previous page" })).toHaveAttribute(
      "href",
      "/app/games?page=16",
    );
  });
});

describe("games list", () => {
  it('reads "Showing 26–50 of 412 games" for page 2', () => {
    expect(showingText({ total: 412, page: 2, pageCount: 17 })).toBe("Showing 26–50 of 412 games");
  });

  it("reads Showing 1 game, and the whole count on one page", () => {
    expect(showingText({ total: 1, page: 1, pageCount: 1 })).toBe("Showing 1 game");
    expect(showingText({ total: 12, page: 1, pageCount: 1 })).toBe("Showing 12 games");
    expect(showingText({ total: 1234, page: 50, pageCount: 50 })).toBe(
      "Showing 1,226–1,234 of 1,234 games",
    );
  });

  it("groups by day under date headings when sorted by date", () => {
    const rows = [
      toRowView(row()),
      toRowView(row({ id: "00000000-0000-4000-8000-000000000002", result: "loss" })),
      toRowView(
        row({
          id: "00000000-0000-4000-8000-000000000003",
          playedAt: new Date("2026-11-10T09:00:00Z"),
        }),
      ),
    ];
    render(<GamesList rows={rows} grouped />);
    const table = screen.getByRole("table");
    const heads = within(table)
      .getAllByRole("columnheader")
      .map((h) => h.textContent ?? "");
    expect(heads[0]).toMatch(/^Result/);
    expect(heads.slice(1, 5)).toEqual(["Opponent", "Time control", "Opening", "Accuracy"]);
    expect(heads[5]).toMatch(/^Marks/);
    expect(
      within(table)
        .getAllByRole("rowheader")
        .map((h) => h.textContent),
    ).toEqual(["12 Nov 2026", "10 Nov 2026"]);
    expect(within(table).getAllByRole("link")).toHaveLength(3);
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual([
      "12 Nov 2026",
      "10 Nov 2026",
    ]);
  });

  it("keeps a Date column, and no day headings, under any other sort", () => {
    render(<GamesList rows={[toRowView(row())]} grouped={false} />);
    const table = screen.getByRole("table");
    expect(within(table).getAllByRole("columnheader")[0]).toHaveTextContent("Date");
    expect(within(table).queryAllByRole("rowheader")).toHaveLength(0);
    expect(screen.queryAllByRole("heading", { level: 2 })).toHaveLength(0);
  });

  it("shows the no-match state with Clear filters", () => {
    render(
      <NoMatch
        description="Classical games"
        suggestions={[]}
        clearHref="/app/games?sort=accuracy-desc"
      />,
    );
    expect(screen.getByText("No games match these filters.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Clear filters" })).toHaveAttribute(
      "href",
      "/app/games?sort=accuracy-desc",
    );
    expect(screen.queryByRole("table")).toBeNull();
  });
});

describe("result mark and colour", () => {
  it("marks the score in the colour you played, and says so", () => {
    const white = toRowView(row());
    const black = toRowView(row({ playerColor: "black", result: "loss" }));
    const draw = toRowView(row({ result: "draw" }));
    expect([white.mark, black.mark, draw.mark]).toEqual(["1", "0", "½"]);
    render(
      <ul>
        <GameRow row={white} />
        <GameRow row={black} />
      </ul>,
    );
    const [a, b] = screen.getAllByRole("link");
    expect(a).toHaveAccessibleName(expect.stringContaining("you played White"));
    expect(b).toHaveAccessibleName(expect.stringContaining("you played Black"));
    expect(screen.getAllByTitle("You played White")).toHaveLength(1);
    expect(screen.getAllByTitle("You played Black")).toHaveLength(1);
  });

  it("names a pending game's status in the link", () => {
    render(
      <ul>
        <GameRow
          row={toRowView(row({ analysisStatus: "pending", accuracy: null, counts: null }))}
        />
      </ul>,
    );
    expect(screen.getByRole("link")).toHaveAccessibleName(
      expect.stringContaining("Waiting for analysis"),
    );
  });

  it("the table link carries the whole game too", () => {
    render(<GamesList rows={[toRowView(row())]} grouped />);
    const link = within(screen.getByRole("table")).getByRole("link");
    expect(link).toHaveAccessibleName(expect.stringContaining("2 blunders, 1 mistake"));
    expect(link).toHaveAccessibleName(expect.stringContaining("you played White"));
  });
});

describe("empty states", () => {
  const accounts = [{ id: "00000000-0000-4000-8000-0000000000aa", label: "chess.com someplayer" }];

  it("words the active filters", () => {
    expect(describeFilters(q({ tc: "classical", result: "draw" }).filters, accounts)).toBe(
      "Classical draws",
    );
    expect(
      describeFilters(
        q({
          colour: "white",
          from: "2026-10-01",
          to: "2026-11-20",
          account: "00000000-0000-4000-8000-0000000000aa",
        }).filters,
        accounts,
      ),
    ).toBe("games as White on chess.com someplayer between 1 Oct 2026 and 20 Nov 2026");
    expect(describeFilters(q({ result: "win", from: "2026-10-01" }).filters, accounts)).toBe(
      "wins since 1 Oct 2026",
    );
    expect(describeFilters(q({ to: "2026-10-01" }).filters, accounts)).toBe(
      "games up to 1 Oct 2026",
    );
  });

  it("no match names the filters and offers the nearest wider lists", () => {
    render(
      <NoMatch
        description="Classical draws"
        clearHref="/app/games"
        suggestions={[{ label: "Any time control", total: 16, href: "/app/games?result=draw" }]}
      />,
    );
    expect(screen.getByText("No games match these filters.")).toBeInTheDocument();
    expect(screen.getByText("You asked for Classical draws.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Any time control: 16 games" })).toHaveAttribute(
      "href",
      "/app/games?result=draw",
    );
  });

  it("no games: importing, failed, or an account that truly has none", () => {
    const { unmount } = render(
      <NoGames importState="importing" accounts={["chess.com someplayer"]} />,
    );
    expect(screen.getByText(/Importing your games from chess.com someplayer/)).toBeInTheDocument();
    unmount();
    const failed = render(<NoGames importState="failed" accounts={["chess.com someplayer"]} />);
    expect(screen.getByText("Import didn't finish. Try again from Accounts.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Go to Accounts" })).toHaveAttribute(
      "href",
      "/app/accounts",
    );
    expect(screen.queryByText(/^Importing/)).toBeNull();
    failed.unmount();
    render(<NoGames importState="done" accounts={["chess.com someplayer"]} />);
    expect(screen.getByText(/chess.com someplayer has no games yet/)).toBeInTheDocument();
    expect(screen.queryByText(/Importing/)).toBeNull();
  });
});

describe("active filter chips (phones)", () => {
  it("lists each active filter as a removable chip", async () => {
    const user = userEvent.setup();
    render(
      <FilterBar query={q({ tc: "rapid", result: "loss", from: "2026-10-01" })} accounts={[]} />,
    );
    const chips = screen.getByRole("list", { name: "Active filters" });
    expect(
      within(chips)
        .getAllByRole("button")
        .map((b) => b.getAttribute("aria-label")),
    ).toEqual(["Remove filter: Rapid", "Remove filter: Losses", "Remove filter: From 1 Oct 2026"]);
    await user.click(screen.getByRole("button", { name: "Remove filter: Losses" }));
    expect(replace).toHaveBeenCalledWith("/app/games?tc=rapid&from=2026-10-01", { scroll: false });
  });

  it("shows no chip list without filters", () => {
    render(<FilterBar query={q({})} accounts={[]} />);
    expect(screen.queryByRole("list", { name: "Active filters" })).toBeNull();
  });
});

describe("fix round 2", () => {
  it("explains the result square on phones and in the table head", () => {
    render(<GamesList rows={[toRowView(row())]} grouped />);
    const legend = "The square is the colour you played; the figure is your score.";
    expect(screen.getAllByText(legend).length).toBeGreaterThanOrEqual(1);
    const head = within(screen.getByRole("table")).getAllByRole("columnheader")[0];
    expect(head).toHaveTextContent(/^Result/);
    expect(head).toHaveTextContent("Square: your colour");
  });

  // M5a T13 critic I4: on phones "3?? 0?" had no key; the table head's key was desktop-only.
  it("keys the ?? and ? marks on phones too", () => {
    const { container } = render(<GamesList rows={[toRowView(row())]} grouped />);
    const phone = container.querySelector("[class~='md:hidden']");
    const key = phone?.querySelector("[data-part='marks-key']");
    expect(key).toHaveTextContent("?? blunder, ? mistake");
  });

  it("keeps result, clock and date whole, truncating only the opening", () => {
    render(
      <ul>
        <GameRow row={toRowView(row())} />
      </ul>,
    );
    const whole = screen.getByText((_, el) => el?.getAttribute("data-part") === "facts");
    expect(whole).toHaveTextContent("Win, Rapid 10+0, 12 Nov 2026");
    expect(whole).toHaveClass("whitespace-nowrap", "shrink-0");
    const opening = screen.getByText((_, el) => el?.getAttribute("data-part") === "opening");
    expect(opening).toHaveClass("min-w-0", "truncate");
  });

  it("moves focus to the next chip after one is removed, then to Filters", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <FilterBar query={q({ tc: "rapid", result: "loss", from: "2026-10-01" })} accounts={[]} />,
    );
    await user.click(screen.getByRole("button", { name: "Remove filter: Losses" }));
    rerender(<FilterBar query={q({ tc: "rapid", from: "2026-10-01" })} accounts={[]} />);
    expect(screen.getByRole("button", { name: "Remove filter: From 1 Oct 2026" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Remove filter: From 1 Oct 2026" }));
    rerender(<FilterBar query={q({ tc: "rapid" })} accounts={[]} />);
    expect(screen.getByRole("button", { name: "Remove filter: Rapid" })).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Remove filter: Rapid" }));
    rerender(<FilterBar query={q({})} accounts={[]} />);
    expect(screen.getByRole("button", { name: /^Filters/ })).toHaveFocus();
  });
});
