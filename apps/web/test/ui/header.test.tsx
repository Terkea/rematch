// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "next-themes";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubMatchMedia } from "./helpers.js";

let pathname = "/app/accounts";
let search = new URLSearchParams();
const push = vi.fn();
vi.mock("next/navigation", () => ({
  usePathname: () => pathname,
  useSearchParams: () => search,
  useRouter: () => ({ push, replace: vi.fn(), refresh: vi.fn() }),
}));
const signOut = vi.fn().mockResolvedValue({ data: { success: true }, error: null });
vi.mock("../../src/lib/auth-client.js", () => ({ signOut: () => signOut() }));
const chatLive = vi.fn(() => true);
vi.mock("../../src/lib/coach-chat.js", () => ({ coachChatLive: () => chatLive() }));

const { Header } = await import("../../src/app/(app)/_shell/header.js");
const { navItems } = await import("../../src/app/(app)/_shell/nav.js");
const { Toaster } = await import("@ci/ui/overlays");

beforeEach(() => {
  stubMatchMedia();
  pathname = "/app/accounts";
  search = new URLSearchParams();
  push.mockReset();
  signOut.mockClear();
  signOut.mockResolvedValue({ data: { success: true }, error: null });
});

function renderHeader() {
  return render(
    <ThemeProvider attribute="data-theme" defaultTheme="light" enableSystem={false}>
      <Header email="adult@example.invalid" signOutTo={vi.fn()} />
    </ThemeProvider>,
  );
}

describe("the signed-in identity", () => {
  it("never shows a Lichess placeholder as an email; it names the Lichess account", async () => {
    render(
      <ThemeProvider attribute="data-theme" defaultTheme="light" enableSystem={false}>
        <Header
          email="fakeuser@lichess.chess-improver.invalid"
          name="FakeUser"
          signOutTo={vi.fn()}
        />
      </ThemeProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    expect(await screen.findByText("Signed in with Lichess")).toBeInTheDocument();
    expect(screen.getByText("FakeUser")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("invalid");
    expect(document.body.textContent).not.toContain("@");
  });

  it("shows a real address as it is", async () => {
    render(
      <ThemeProvider attribute="data-theme" defaultTheme="light" enableSystem={false}>
        <Header email="adult@example.invalid" name="Ana" signOutTo={vi.fn()} />
      </ThemeProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    expect(await screen.findByText("Signed in as")).toBeInTheDocument();
    expect(screen.getByText("adult@example.invalid")).toBeInTheDocument();
  });
});

describe("isPlaceholderEmail", () => {
  it("is true only for the Lichess placeholder domain", async () => {
    const { isPlaceholderEmail, placeholderEmail } = await import(
      "../../src/lib/placeholder-email.js"
    );
    expect(isPlaceholderEmail(placeholderEmail("FakeUser"))).toBe(true);
    expect(placeholderEmail("FakeUser")).toBe("fakeuser@lichess.chess-improver.invalid");
    expect(isPlaceholderEmail("BOB@LICHESS.CHESS-IMPROVER.INVALID")).toBe(true);
    expect(isPlaceholderEmail("adult@example.invalid")).toBe(false);
    expect(isPlaceholderEmail("x@notlichess.chess-improver.invalid")).toBe(false);
  });
});

describe("shell header", () => {
  it("lists the sections, matching the pages' titles (order pinned in test/nav.test.ts)", () => {
    expect(navItems.map((i) => i.label)).toEqual([
      "Overview",
      "Games",
      "Report",
      "Weaknesses",
      "Train",
      "Strengths",
      "Progress",
      "Coach",
      "Accounts",
    ]);
    // "dashboard" stays a command-palette keyword for the old name.
    expect(navItems[0].keywords).toContain("dashboard");
  });

  it("has the wordmark, the nav with the current page, and the activity slot", () => {
    renderHeader();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("link", { name: "Rematch" })).toBeInTheDocument();
    const nav = within(banner).getByRole("navigation", { name: "Main" });
    expect(within(nav).getByRole("link", { name: "Accounts" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Games" })).not.toHaveAttribute("aria-current");
    expect(banner.querySelectorAll('[data-slot="activity"]')).toHaveLength(1);
  });

  it("never wraps the wordmark; the activity slot gives way instead (I7)", () => {
    renderHeader();
    const banner = screen.getByRole("banner");
    const mark = within(banner).getByRole("link", { name: "Rematch" });
    expect(mark.className).toMatch(/(^| )whitespace-nowrap( |$)/);
    expect(mark.className).toMatch(/(^| )shrink-0( |$)/);
    const slot = banner.querySelector('[data-slot="activity"]');
    expect(slot?.className).toMatch(/(^| )min-w-0( |$)/);
    const group = slot?.parentElement;
    expect(group?.className).toMatch(/(^| )min-w-0( |$)/);
    // A zero basis: the group never wraps under the wordmark on a phone; it takes what is left.
    expect(group?.className).toMatch(/(^| )flex-1( |$)/);
    expect(group?.className).toMatch(/(^| )basis-0( |$)/);
    // The controls keep their size; only the activity line gives way.
    const search = within(banner).getByRole("button", { name: /Search/ });
    expect(search.closest(".shrink-0")).not.toBeNull();
    // Beside the one-row nav the search button is its icon and Ctrl K; the word stays for AT.
    expect(within(search).getByText("Search").className).toMatch(/(^| )md:sr-only( |$)/);
    // While background work shows there, Ctrl K steps aside too, so its count has room.
    expect(within(search).getByText("Ctrl K").className).toContain(
      "md:group-has-[[data-slot=activity-indicator]]/right:hidden",
    );
  });

  it("overflows the sections that don't fit a phone row into the account menu", async () => {
    pathname = "/app/progress";
    renderHeader();
    const nav = screen.getByRole("navigation", { name: "Main" });
    // Every section stays in the header nav (shown from lg up); only these four show below lg.
    const phone = within(nav)
      .getAllByRole("link")
      .filter((a) => !a.closest("li")?.className.includes("max-lg:hidden"))
      .map((a) => a.textContent);
    expect(phone).toEqual(["Overview", "Games", "Report", "Coach"]);
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    const more = await screen.findByRole("navigation", { name: "More pages" });
    expect(more.className).toContain("lg:hidden");
    expect(
      within(more)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Weaknesses", "Train", "Strengths", "Progress", "Accounts"]);
    expect(within(more).getByRole("link", { name: "Progress" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("marks a nested games route as Games", () => {
    pathname = "/app/games/123";
    renderHeader();
    expect(screen.getByRole("link", { name: "Games" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Overview" })).not.toHaveAttribute("aria-current");
  });

  it("toggles the theme with an explicit label", async () => {
    renderHeader();
    const toDark = await screen.findByRole("button", { name: "Use dark theme" });
    await userEvent.click(toDark);
    expect(await screen.findByRole("button", { name: "Use light theme" })).toBeInTheDocument();
  });

  it("opens the command menu from its trigger, which shows Ctrl K", async () => {
    renderHeader();
    const trigger = screen.getByRole("button", { name: /Search/ });
    expect(within(trigger).getByText("Ctrl K")).toBeInTheDocument();
    await userEvent.click(trigger);
    const menu = await screen.findByRole("dialog");
    expect(within(menu).getByText("Go to accounts")).toBeInTheDocument();
    expect(within(menu).getByText("Sign out")).toBeInTheDocument();
  });

  it("offers Go to overview and Go to games in the command menu", async () => {
    renderHeader();
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    const menu = await screen.findByRole("dialog");
    await userEvent.click(within(menu).getByText("Go to overview"));
    expect(push).toHaveBeenLastCalledWith("/app");
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    await userEvent.click(within(await screen.findByRole("dialog")).getByText("Go to games"));
    expect(push).toHaveBeenLastCalledWith("/app/games");
  });

  it("keeps the page's filters on the header and command-menu links (audit A7)", async () => {
    pathname = "/app/weaknesses";
    search = new URLSearchParams("account=acc-1&tc=blitz&color=black&from=2026-09-01");
    renderHeader();
    const nav = within(screen.getByRole("navigation", { name: "Main" }));
    expect(nav.getByRole("link", { name: "Report" })).toHaveAttribute(
      "href",
      "/app/report?account=acc-1&tc=blitz&color=black&from=2026-09-01",
    );
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    const menu = await screen.findByRole("dialog");
    await userEvent.click(within(menu).getByText("Endgame"));
    expect(push).toHaveBeenLastCalledWith(
      "/app/areas/endgame?account=acc-1&tc=blitz&color=black&from=2026-09-01",
    );
  });

  it("groups the seven skill areas in the command menu, Endgame by its name", async () => {
    renderHeader();
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    const menu = await screen.findByRole("dialog");
    const group = within(menu).getByRole("group", { name: "Skill areas" });
    expect(
      within(group)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Openings", "Middlegame", "Endgame", "Tactics", "Calculation", "Time", "Mindset"]);
    await userEvent.click(within(group).getByRole("option", { name: "Endgame" }));
    expect(push).toHaveBeenLastCalledWith("/app/areas/endgame");
  });

  it("hides Coach and Ask your coach while the chat pages are not live", async () => {
    chatLive.mockReturnValue(false);
    try {
      renderHeader();
      expect(screen.queryByRole("link", { name: "Coach" })).toBeNull();
      await userEvent.click(screen.getByRole("button", { name: /Search/ }));
      const menu = await screen.findByRole("dialog");
      expect(within(menu).queryByRole("option", { name: "Ask your coach" })).toBeNull();
      expect(within(menu).queryByText("Go to coach")).toBeNull();
    } finally {
      chatLive.mockReturnValue(true);
    }
  });

  it("offers Ask your coach, which opens a new thread", async () => {
    renderHeader();
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    const menu = await screen.findByRole("dialog");
    await userEvent.click(within(menu).getByRole("option", { name: "Ask your coach" }));
    expect(push).toHaveBeenLastCalledWith("/app/coach/new");
  });

  it("reaches the licences from the command menu (review-t27 M2)", async () => {
    renderHeader();
    await userEvent.click(screen.getByRole("button", { name: /Search/ }));
    await userEvent.click(within(await screen.findByRole("dialog")).getByText("Go to licences"));
    expect(push).toHaveBeenLastCalledWith("/licenses");
  });

  it("signs out from the user menu", async () => {
    const signOutTo = vi.fn();
    render(
      <ThemeProvider attribute="data-theme" defaultTheme="light" enableSystem={false}>
        <Header email="adult@example.invalid" signOutTo={signOutTo} />
      </ThemeProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    expect(await screen.findByText("adult@example.invalid")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(signOut).toHaveBeenCalled();
    expect(signOutTo).toHaveBeenCalledWith("/sign-in");
  });

  it("stays signed in and says so when signing out fails", async () => {
    signOut.mockResolvedValue({ data: null, error: { status: 500 } });
    const signOutTo = vi.fn();
    render(
      <ThemeProvider attribute="data-theme" defaultTheme="light" enableSystem={false}>
        <Header email="adult@example.invalid" signOutTo={signOutTo} />
        <Toaster />
      </ThemeProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Account menu" }));
    await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
    expect(await screen.findByText("Couldn't sign out. Try again.")).toBeInTheDocument();
    expect(signOutTo).not.toHaveBeenCalled();
  });
});
