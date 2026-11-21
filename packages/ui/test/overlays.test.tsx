import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandMenu, Drawer, Toaster, toast } from "../src/overlays/index.js";

afterEach(cleanup);

// cmdk and vaul measure with APIs jsdom lacks.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= () => {};
window.matchMedia ??= ((query: string) => ({
  matches: false,
  media: query,
  addEventListener() {},
  removeEventListener() {},
  addListener() {},
  removeListener() {},
  dispatchEvent: () => false,
  onchange: null,
})) as typeof window.matchMedia;

const onSelect = vi.fn();
const items = [
  { id: "accounts", label: "Accounts", group: "Pages", onSelect },
  { id: "games", label: "Games", group: "Pages", keywords: ["history"], onSelect: vi.fn() },
  { id: "drills", label: "Start a drill", group: "Actions", onSelect: vi.fn() },
];

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button">outside</button>
      <CommandMenu items={items} open={open} onOpenChange={setOpen} />
    </>
  );
}

describe("CommandMenu", () => {
  it("opens on Ctrl+K and focuses the search field", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    expect(screen.queryByPlaceholderText("Search pages and actions")).toBeNull();
    await user.keyboard("{Control>}k{/Control}");
    const input = await screen.findByPlaceholderText("Search pages and actions");
    await waitFor(() => expect(input).toHaveFocus());
  });

  it("opens on Cmd+K", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("{Meta>}k{/Meta}");
    expect(await screen.findByPlaceholderText("Search pages and actions")).toBeInTheDocument();
  });

  it("filters by typing and by keywords", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("acc");
    expect(screen.getByRole("option", { name: "Accounts" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Games" })).toBeNull();
    await user.clear(screen.getByPlaceholderText("Search pages and actions"));
    await user.keyboard("history");
    expect(screen.getByRole("option", { name: "Games" })).toBeInTheDocument();
  });

  it("Enter selects the highlighted item and closes", async () => {
    onSelect.mockClear();
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("acc{Enter}");
    expect(onSelect).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(screen.queryByPlaceholderText("Search pages and actions")).toBeNull(),
    );
  });

  it("ArrowDown moves the highlight, then Enter selects that item", async () => {
    const games = items[1]?.onSelect as ReturnType<typeof vi.fn>;
    games.mockClear();
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByPlaceholderText("Search pages and actions");
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("option", { name: "Games" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Enter}");
    expect(games).toHaveBeenCalledTimes(1);
  });

  it("Escape closes it and returns focus to the page", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    screen.getByRole("button", { name: "outside" }).focus();
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByPlaceholderText("Search pages and actions");
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByPlaceholderText("Search pages and actions")).toBeNull(),
    );
    expect(screen.getByRole("button", { name: "outside" })).toHaveFocus();
  });

  it("shows an empty state when nothing matches", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("zzzz");
    expect(screen.getByText("No match")).toBeInTheDocument();
  });
});

describe("Toaster", () => {
  it("renders a toast's text", async () => {
    render(<Toaster />);
    act(() => {
      toast("chess.com connected");
    });
    expect(await screen.findByText("chess.com connected")).toBeInTheDocument();
  });
});

describe("Drawer", () => {
  it("opens from its trigger, names itself, and closes on Escape returning focus", async () => {
    const user = userEvent.setup();
    render(
      <Drawer.Root>
        <Drawer.Trigger>Open filters</Drawer.Trigger>
        <Drawer.Content>
          <Drawer.Title>Filters</Drawer.Title>
          <Drawer.Description>Narrow the game list.</Drawer.Description>
          <button type="button">Apply</button>
        </Drawer.Content>
      </Drawer.Root>,
    );
    const trigger = screen.getByRole("button", { name: "Open filters" });
    await user.click(trigger);
    const dialog = await screen.findByRole("dialog", { name: "Filters" });
    expect(dialog).toHaveClass("rounded-t-sheet", "shadow-lift");
    await user.keyboard("{Escape}");
    // Vaul unmounts on transitionend, which jsdom never fires: the closed state is the contract.
    await waitFor(() => expect(trigger).toHaveAttribute("aria-expanded", "false"));
    expect(dialog).toHaveAttribute("data-state", "closed");
    await waitFor(() => expect(trigger).toHaveFocus());
  });
});
