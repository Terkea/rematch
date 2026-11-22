// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "next-themes";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubMatchMedia } from "./helpers.js";

let pathname = "/app/accounts";
const push = vi.fn();
vi.mock("next/navigation", () => ({
  usePathname: () => pathname,
  useRouter: () => ({ push, replace: vi.fn(), refresh: vi.fn() }),
}));
const signOut = vi.fn().mockResolvedValue({ data: { success: true }, error: null });
vi.mock("../../src/lib/auth-client.js", () => ({ signOut: () => signOut() }));

const { Header } = await import("../../src/app/(app)/_shell/header.js");
const { navItems } = await import("../../src/app/(app)/_shell/nav.js");
const { Toaster } = await import("@ci/ui/overlays");

beforeEach(() => {
  stubMatchMedia();
  pathname = "/app/accounts";
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

describe("shell header", () => {
  it("lists Overview, Games and Accounts, matching the pages' titles", () => {
    expect(navItems.map((i) => [i.label, i.href])).toEqual([
      ["Overview", "/app"],
      ["Games", "/app/games"],
      ["Accounts", "/app/accounts"],
    ]);
    // "dashboard" stays a command-palette keyword for the old name.
    expect(navItems[0].keywords).toContain("dashboard");
  });

  it("has the wordmark, the nav with the current page, and the activity slot", () => {
    renderHeader();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("link", { name: "chess-improver" })).toBeInTheDocument();
    const nav = within(banner).getByRole("navigation", { name: "Main" });
    expect(within(nav).getByRole("link", { name: "Accounts" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByRole("link", { name: "Games" })).not.toHaveAttribute("aria-current");
    expect(banner.querySelectorAll('[data-slot="activity"]')).toHaveLength(1);
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
